// Package jsonrpc implements the NDJSON JSON-RPC 2.0 transport between the
// desktop Electron main process and the agent sidecar. One message per line,
// in both directions; a single Conn serves inbound requests and sends outbound
// requests (host.proxy) over the same stream.
package jsonrpc

import (
	"bufio"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"strconv"
	"sync"
)

// Version is the JSON-RPC protocol version carried by every message.
const Version = "2.0"

// Handler serves one inbound request. The returned value is marshalled as the
// response result; returning an error produces a JSON-RPC error response.
type Handler func(ctx context.Context, params json.RawMessage) (any, error)

// NotificationHandler serves one inbound notification. It has no reply.
type NotificationHandler func(params json.RawMessage)

type message struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id,omitempty"`
	Method  string          `json:"method,omitempty"`
	Params  json.RawMessage `json:"params,omitempty"`
	Result  json.RawMessage `json:"result,omitempty"`
	Error   *Error          `json:"error,omitempty"`
}

// Conn is a bidirectional JSON-RPC connection over a byte stream.
type Conn struct {
	in  io.Reader
	out io.Writer

	writeMu sync.Mutex

	mu        sync.Mutex
	handlers  map[string]Handler
	notifiers map[string]NotificationHandler
	pending   map[string]chan *message
	nextID    uint64
	closed    bool
	closeErr  error

	done      chan struct{}
	closeOnce sync.Once
	inflight  sync.WaitGroup
}

// NewConn wires a connection to a stream pair.
func NewConn(in io.Reader, out io.Writer) *Conn {
	return &Conn{
		in:        in,
		out:       out,
		handlers:  map[string]Handler{},
		notifiers: map[string]NotificationHandler{},
		pending:   map[string]chan *message{},
		done:      make(chan struct{}),
	}
}

// Handle registers the request handler for method.
func (c *Conn) Handle(method string, handler Handler) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.handlers[method] = handler
}

// HandleNotification registers the notification handler for method.
func (c *Conn) HandleNotification(method string, handler NotificationHandler) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.notifiers[method] = handler
}

// Done is closed when the connection is closed.
func (c *Conn) Done() <-chan struct{} { return c.done }

// Serve reads messages until the input ends or fails.
func (c *Conn) Serve() error {
	reader := bufio.NewReaderSize(c.in, 64*1024)
	for {
		line, err := reader.ReadBytes('\n')
		if len(bytes.TrimSpace(line)) > 0 {
			c.dispatch(line)
		}
		if err != nil {
			if errors.Is(err, io.EOF) {
				c.Close(nil)
				return nil
			}
			c.Close(err)
			return err
		}
	}
}

// Close stops the connection and fails every pending outbound call.
func (c *Conn) Close(err error) {
	c.closeOnce.Do(func() {
		c.mu.Lock()
		c.closed = true
		if err != nil {
			c.closeErr = err
		}
		pending := c.pending
		c.pending = map[string]chan *message{}
		c.mu.Unlock()

		for id, ch := range pending {
			ch <- &message{Error: &Error{
				Code:    CodeInternal,
				Message: "connection closed: " + id,
			}}
		}
		close(c.done)
		c.inflight.Wait()
	})
}

// Call sends a request and waits for its response, the caller's context to
// end, or the connection to close.
func (c *Conn) Call(ctx context.Context, method string, params any) (json.RawMessage, error) {
	raw, err := json.Marshal(params)
	if err != nil {
		return nil, err
	}

	rawID := json.RawMessage(strconv.Quote(c.newRequestID()))
	key := string(rawID)
	waiter := make(chan *message, 1)

	c.mu.Lock()
	if c.closed {
		failure := c.closeErr
		c.mu.Unlock()
		if failure == nil {
			failure = errors.New("connection closed")
		}
		return nil, failure
	}
	c.pending[key] = waiter
	c.mu.Unlock()

	request := &message{JSONRPC: Version, ID: rawID, Method: method, Params: raw}
	if err := c.write(request); err != nil {
		c.removePending(key)
		return nil, err
	}

	select {
	case response := <-waiter:
		if response.Error != nil {
			return nil, response.Error
		}
		return response.Result, nil
	case <-ctx.Done():
		c.removePending(key)
		return nil, ctx.Err()
	case <-c.done:
		c.removePending(key)
		c.mu.Lock()
		failure := c.closeErr
		c.mu.Unlock()
		if failure == nil {
			failure = errors.New("connection closed")
		}
		return nil, failure
	}
}

// Notify sends a notification. Notifications carry no reply.
func (c *Conn) Notify(method string, params any) error {
	raw, err := json.Marshal(params)
	if err != nil {
		return err
	}
	return c.write(&message{JSONRPC: Version, Method: method, Params: raw})
}

func (c *Conn) dispatch(line []byte) {
	var msg message
	if err := json.Unmarshal(bytes.TrimSpace(line), &msg); err != nil {
		return
	}

	if hasID(msg.ID) && (msg.Result != nil || msg.Error != nil) {
		c.deliver(&msg)
		return
	}
	if msg.Method == "" {
		return
	}

	params := append(json.RawMessage(nil), msg.Params...)

	if hasID(msg.ID) {
		id := append(json.RawMessage(nil), msg.ID...)
		method := msg.Method
		c.inflight.Add(1)
		go func() {
			defer c.inflight.Done()
			result, err := c.invoke(method, params)
			c.respond(id, result, err)
		}()
		return
	}

	c.mu.Lock()
	notifier := c.notifiers[msg.Method]
	c.mu.Unlock()
	if notifier == nil {
		return
	}
	c.inflight.Add(1)
	go func() {
		defer c.inflight.Done()
		notifier(params)
	}()
}

func (c *Conn) invoke(method string, params json.RawMessage) (any, error) {
	c.mu.Lock()
	handler := c.handlers[method]
	c.mu.Unlock()
	if handler == nil {
		return nil, MethodNotFound(method)
	}
	return handler(context.Background(), params)
}

func (c *Conn) respond(id json.RawMessage, result any, err error) {
	if err != nil {
		failure := &Error{Code: CodeInternal, Message: err.Error()}
		var rpcErr *Error
		if errors.As(err, &rpcErr) {
			failure = rpcErr
		}
		_ = c.write(&message{JSONRPC: Version, ID: id, Error: failure})
		return
	}

	raw, marshalErr := json.Marshal(result)
	if marshalErr != nil {
		_ = c.write(&message{JSONRPC: Version, ID: id, Error: &Error{
			Code:    CodeInternal,
			Message: marshalErr.Error(),
		}})
		return
	}
	_ = c.write(&message{JSONRPC: Version, ID: id, Result: raw})
}

func (c *Conn) deliver(msg *message) {
	id := string(bytes.TrimSpace(msg.ID))
	c.mu.Lock()
	waiter := c.pending[id]
	delete(c.pending, id)
	c.mu.Unlock()
	if waiter == nil {
		return
	}
	waiter <- msg
}

func (c *Conn) removePending(id string) {
	c.mu.Lock()
	delete(c.pending, id)
	c.mu.Unlock()
}

func (c *Conn) newRequestID() string {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.nextID++
	return "srv-" + strconv.FormatUint(c.nextID, 10)
}

func (c *Conn) write(msg *message) error {
	raw, err := json.Marshal(msg)
	if err != nil {
		return err
	}
	c.writeMu.Lock()
	defer c.writeMu.Unlock()
	_, err = c.out.Write(append(raw, '\n'))
	return err
}

func hasID(id json.RawMessage) bool {
	trimmed := bytes.TrimSpace(id)
	return len(trimmed) > 0 && !bytes.Equal(trimmed, []byte("null"))
}

// Decode unmarshals request params into T. Absent or null params decode to the
// zero value.
func Decode[T any](params json.RawMessage) (T, error) {
	var value T
	trimmed := bytes.TrimSpace(params)
	if len(trimmed) == 0 || bytes.Equal(trimmed, []byte("null")) {
		return value, nil
	}
	if err := json.Unmarshal(trimmed, &value); err != nil {
		return value, InvalidParams("%v", err)
	}
	return value, nil
}

// Error codes. The application range matches what host-core and the Node
// sidecar already send.
const (
	CodeParseError     = -32700
	CodeInvalidRequest = -32600
	CodeMethodNotFound = -32601
	CodeInvalidParams  = -32602
	CodeInternal       = -32603
	CodeApplication    = -32000
)

// Error is the JSON-RPC error object. Data carries the machine-readable
// errorCode the renderer keys off.
type Error struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
	Data    any    `json:"data,omitempty"`
}

func (e *Error) Error() string { return e.Message }

// Errorf builds an error with an explicit code.
func Errorf(code int, format string, args ...any) *Error {
	return &Error{Code: code, Message: fmt.Sprintf(format, args...)}
}

// InvalidParams reports a request whose params could not be decoded.
func InvalidParams(format string, args ...any) *Error {
	return Errorf(CodeInvalidParams, format, args...)
}

// AppError reproduces the Node sidecar's application error shape: code -32000
// with data.errorCode.
func AppError(errorCode, format string, args ...any) *Error {
	return &Error{
		Code:    CodeApplication,
		Message: fmt.Sprintf(format, args...),
		Data:    map[string]string{"errorCode": errorCode},
	}
}

// MethodNotFound mirrors the Node sidecar's reply for an unknown method.
func MethodNotFound(method string) *Error {
	return &Error{
		Code:    CodeMethodNotFound,
		Message: "method not found: " + method,
		Data:    map[string]string{"errorCode": "INTERNAL"},
	}
}

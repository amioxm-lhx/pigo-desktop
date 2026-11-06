package server

import (
	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
)

// eventEmitter streams agent events to Electron main as agent.event
// notifications.
type eventEmitter struct {
	conn *jsonrpc.Conn
}

func newEventEmitter(conn *jsonrpc.Conn) *eventEmitter {
	return &eventEmitter{conn: conn}
}

// Emit publishes one envelope.
func (e *eventEmitter) Emit(envelope protocol.EventEnvelope) error {
	return e.conn.Notify(protocol.MethodAgentEvent, envelope)
}

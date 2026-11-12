package im

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"
)

// bindingVersion is the on-disk schema version. It exists so a future shape can
// migrate rather than guess.
const bindingVersion = 1

// Binding records which pigo session a chat is currently driving.
type Binding struct {
	// SessionID is the pigo session id (session.NewID form).
	SessionID string `json:"session_id"`
	// Cwd is the working directory the session runs in. It is fixed at bind
	// time on purpose: letting a chat message choose the directory would hand
	// anyone who can post in the chat the whole filesystem.
	Cwd string `json:"cwd,omitempty"`
	// Model is the model the session was last switched to.
	Model string `json:"model,omitempty"`
	// ChatTitle is a human label, stored only to make `im bindings` readable.
	ChatTitle string `json:"chat_title,omitempty"`
	// CreatedAt and LastActive bound the record's usefulness for cleanup.
	CreatedAt  time.Time `json:"created_at"`
	LastActive time.Time `json:"last_active"`
}

// bindingFile is the JSON document persisted to disk.
type bindingFile struct {
	Version  int                `json:"version"`
	Bindings map[string]Binding `json:"bindings"`
}

// BindingStore persists chat → session bindings.
//
// It is deliberately file-backed rather than in-memory: a binding that vanished
// on restart would orphan every session the user had been driving from their
// phone, and "which conversation was I in" is exactly the thing they cannot
// reconstruct from the chat history. Writes are atomic (temp file + rename) so a
// crash mid-write cannot truncate the map.
type BindingStore struct {
	path string

	mu   sync.Mutex
	data bindingFile
}

// NewBindingStore loads the store at path. A missing file is not an error (the
// store starts empty); a malformed one is, because silently discarding the
// user's bindings would look like data loss.
func NewBindingStore(path string) (*BindingStore, error) {
	s := &BindingStore{
		path: path,
		data: bindingFile{Version: bindingVersion, Bindings: map[string]Binding{}},
	}
	if path == "" {
		return s, nil
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return s, nil
		}
		return nil, fmt.Errorf("im: read bindings %s: %w", path, err)
	}
	if len(raw) == 0 {
		return s, nil
	}
	var doc bindingFile
	if err := json.Unmarshal(raw, &doc); err != nil {
		return nil, fmt.Errorf("im: parse bindings %s: %w", path, err)
	}
	if doc.Bindings == nil {
		doc.Bindings = map[string]Binding{}
	}
	if doc.Version == 0 {
		doc.Version = bindingVersion
	}
	s.data = doc
	return s, nil
}

// Path returns the file the store persists to ("" for an in-memory store).
func (s *BindingStore) Path() string { return s.path }

// Get returns the binding for a chat key.
func (s *BindingStore) Get(chatKey string) (Binding, bool) {
	if s == nil {
		return Binding{}, false
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	b, ok := s.data.Bindings[chatKey]
	return b, ok
}

// Put stores (or replaces) the binding for a chat key.
func (s *BindingStore) Put(chatKey string, b Binding) error {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	s.data.Bindings[chatKey] = b
	return s.persistLocked()
}

// Touch refreshes LastActive without changing the rest of the binding.
func (s *BindingStore) Touch(chatKey string, at time.Time) error {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	b, ok := s.data.Bindings[chatKey]
	if !ok {
		return nil
	}
	b.LastActive = at
	s.data.Bindings[chatKey] = b
	return s.persistLocked()
}

// Delete removes a chat's binding. Deleting an absent key is not an error.
func (s *BindingStore) Delete(chatKey string) error {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, ok := s.data.Bindings[chatKey]; !ok {
		return nil
	}
	delete(s.data.Bindings, chatKey)
	return s.persistLocked()
}

// List returns a snapshot of every binding, most recently active first.
func (s *BindingStore) List() map[string]Binding {
	if s == nil {
		return nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	out := make(map[string]Binding, len(s.data.Bindings))
	for k, v := range s.data.Bindings {
		out[k] = v
	}
	return out
}

// Keys returns the chat keys sorted most-recently-active first.
func (s *BindingStore) Keys() []string {
	list := s.List()
	keys := make([]string, 0, len(list))
	for k := range list {
		keys = append(keys, k)
	}
	sort.Slice(keys, func(i, j int) bool {
		if list[keys[i]].LastActive.Equal(list[keys[j]].LastActive) {
			return keys[i] < keys[j]
		}
		return list[keys[i]].LastActive.After(list[keys[j]].LastActive)
	})
	return keys
}

func (s *BindingStore) persistLocked() error {
	if s.path == "" {
		return nil
	}
	s.data.Version = bindingVersion
	raw, err := json.MarshalIndent(s.data, "", "  ")
	if err != nil {
		return fmt.Errorf("im: encode bindings: %w", err)
	}
	if dir := filepath.Dir(s.path); dir != "" {
		if err := os.MkdirAll(dir, 0o700); err != nil {
			return fmt.Errorf("im: create bindings dir %s: %w", dir, err)
		}
	}
	tmp, err := os.CreateTemp(filepath.Dir(s.path), ".bindings-*.tmp")
	if err != nil {
		return fmt.Errorf("im: create temp bindings: %w", err)
	}
	tmpName := tmp.Name()
	defer os.Remove(tmpName)
	if _, err := tmp.Write(raw); err != nil {
		tmp.Close()
		return fmt.Errorf("im: write temp bindings: %w", err)
	}
	if err := tmp.Close(); err != nil {
		return fmt.Errorf("im: close temp bindings: %w", err)
	}
	if err := os.Rename(tmpName, s.path); err != nil {
		return fmt.Errorf("im: replace bindings %s: %w", s.path, err)
	}
	return nil
}

// DefaultBindingPath returns $PIGO_HOME/im/bindings.json, or
// ~/.pigo/im/bindings.json when PIGO_HOME is unset. It mirrors the home
// resolution the credential store uses so both live under one root.
func DefaultBindingPath() string {
	if dir := os.Getenv("PIGO_HOME"); dir != "" {
		return filepath.Join(dir, "im", "bindings.json")
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".pigo", "im", "bindings.json")
}

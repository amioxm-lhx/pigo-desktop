package runtime

import (
	"crypto/rand"
	"encoding/hex"
	"strconv"
	"time"
)

// NewTurnID returns an id for a turn Electron main did not name.
func NewTurnID() string { return newID() }

// newID returns a stable identifier for one assistant transcript row.
func newID() string {
	var buf [12]byte
	if _, err := rand.Read(buf[:]); err == nil {
		return hex.EncodeToString(buf[:])
	}
	return strconv.FormatInt(time.Now().UnixNano(), 16)
}

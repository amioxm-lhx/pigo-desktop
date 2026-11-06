// Command pigo-desktop-agent is the pigo-based agent sidecar for Pi Desktop.
//
// It speaks the same stdio NDJSON JSON-RPC 2.0 protocol as the Node sidecar it
// is meant to replace, so Electron main and the Rust host-core do not have to
// change. This is the minimal closed loop: sidecar.configure/health, one
// session per agent.prompt, host-proxied tools, event streaming, abort/stop.
package main

import (
	"flag"
	"fmt"
	"log"
	"os"

	"github.com/smallnest/pigo/desktop/sidecar/internal/hostproxy"
	"github.com/smallnest/pigo/desktop/sidecar/internal/jsonrpc"
	"github.com/smallnest/pigo/desktop/sidecar/internal/protocol"
	"github.com/smallnest/pigo/desktop/sidecar/internal/server"
)

// version is overridable at build time:
//
//	go build -ldflags "-X main.version=1.2.3" ./...
var version = "0.1.0"

func main() {
	showVersion := flag.Bool("version", false, "print the sidecar version and exit")
	flag.Parse()

	if *showVersion {
		fmt.Println(version)
		return
	}

	logger := log.New(os.Stderr, "[pigo-agent] ", log.LstdFlags|log.Lmicroseconds)

	conn := jsonrpc.NewConn(os.Stdin, os.Stdout)
	host := hostproxy.New(conn)
	srv := server.New(server.Options{Version: version, Logger: logger, Host: host})
	srv.Register(conn)

	logger.Printf("ready version=%s pid=%d protocol=%s", version, os.Getpid(), protocol.MethodHostProxy)
	if err := conn.Serve(); err != nil {
		logger.Printf("exiting after transport error: %v", err)
		os.Exit(1)
	}
	logger.Printf("exiting: stdin closed")
}

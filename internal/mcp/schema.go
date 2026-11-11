// This file sanitizes third-party MCP tool input schemas before registration
// (docs/mcp-local-integration.md §3.4). MCP's inputSchema is a JSON Schema
// subset, but not everything a server declares can be compiled offline by
// santhosh-tekuri/jsonschema/v6 — and a schema that fails compileSchema would
// otherwise be silently swallowed by ToolRegistry.Register. Sanitization plus
// the explicit compile check in the Manager turns that silent drop into a
// loud, per-tool warning.
package mcp

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strings"
)

// emptyObjectSchema is the permissive fallback for an absent/empty schema,
// matching plugin.go's degrade strategy ({"type":"object"} widened with an
// explicit empty properties map — both compile fine).
const emptyObjectSchema = `{"type":"object","properties":{}}`

// SanitizeSchema applies the five §3.4 rules to one tool schema:
//
//  1. empty / null / not valid JSON → the permissive empty-object schema, so a
//     no-argument tool still registers (mirrors plugin.go:165-170);
//  2. "$schema" keys are stripped at every level (drafts a server declares may
//     conflict with the compiler's default);
//  3. any "$ref" pointing outside the document (http://, https://, file://) is
//     rejected as an error — offline compilation cannot resolve it, and it is
//     an SSRF surface; the caller drops the tool with a warning;
//  4. "additionalProperties" is never tightened (a missing key stays missing —
//     third-party servers commonly relax it and adding false would break them);
//  5. a top level that is valid JSON but not an object is an error (drop the
//     tool): arguments are always a JSON object in practice.
func SanitizeSchema(raw json.RawMessage) (json.RawMessage, error) {
	trimmed := bytes.TrimSpace(raw)
	if len(trimmed) == 0 || bytes.Equal(trimmed, []byte("null")) {
		return json.RawMessage(emptyObjectSchema), nil
	}
	var doc any
	if err := json.Unmarshal(trimmed, &doc); err != nil {
		// Undecodable schema: degrade rather than drop — the tool may not need
		// argument validation, and a drop would be silent from the model's view.
		return json.RawMessage(emptyObjectSchema), nil
	}
	if err := sanitizeNode(doc); err != nil {
		return nil, err
	}
	obj, ok := doc.(map[string]any)
	if !ok {
		return nil, fmt.Errorf("mcp: schema top level must be an object, got %T", doc)
	}
	out, err := json.Marshal(obj)
	if err != nil {
		return nil, fmt.Errorf("mcp: re-encode schema: %w", err)
	}
	return out, nil
}

// sanitizeNode walks a decoded schema, stripping $schema keys and rejecting
// external $refs in place. The walk mutates maps (safe: the value tree is
// freshly decoded from the caller's bytes).
func sanitizeNode(node any) error {
	switch v := node.(type) {
	case map[string]any:
		delete(v, "$schema")
		if ref, ok := v["$ref"]; ok {
			if s, isStr := ref.(string); isStr && externalRef(s) {
				return fmt.Errorf("mcp: schema $ref %q points outside the document", s)
			}
		}
		for _, child := range v {
			if err := sanitizeNode(child); err != nil {
				return err
			}
		}
	case []any:
		for _, child := range v {
			if err := sanitizeNode(child); err != nil {
				return err
			}
		}
	}
	return nil
}

// externalRef reports whether a $ref leaves the document. Local refs ("#...",
// "#/definitions/...") stay; anything with a scheme (http/https/file/...) is
// refused.
func externalRef(ref string) bool {
	r := strings.TrimSpace(ref)
	if strings.HasPrefix(r, "#") {
		return false
	}
	for _, scheme := range []string{"http://", "https://", "file://"} {
		if strings.HasPrefix(r, scheme) {
			return true
		}
	}
	return false
}

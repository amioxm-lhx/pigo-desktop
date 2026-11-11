package mcp

import (
	"encoding/json"
	"strings"
	"testing"
)

// TestSanitizeSchemaEmpty covers rule 1: empty / null / undecodable input
// degrades to the permissive empty-object schema so a no-argument tool still
// registers.
func TestSanitizeSchemaEmpty(t *testing.T) {
	for _, in := range []json.RawMessage{
		nil,
		{},
		json.RawMessage("null"),
		json.RawMessage("   "),
		json.RawMessage("{not json"),
	} {
		out, err := SanitizeSchema(in)
		if err != nil {
			t.Fatalf("SanitizeSchema(%q): %v", in, err)
		}
		if string(out) != emptyObjectSchema {
			t.Errorf("SanitizeSchema(%q) = %s, want %s", in, out, emptyObjectSchema)
		}
	}
}

// TestSanitizeSchemaStripsDraft covers rule 2: "$schema" keys are removed at
// every level so a server-declared draft cannot conflict with the compiler.
func TestSanitizeSchemaStripsDraft(t *testing.T) {
	in := json.RawMessage(`{
	  "$schema": "http://json-schema.org/draft-07/schema#",
	  "type": "object",
	  "properties": {
	    "q": {"type": "string", "$schema": "http://json-schema.org/draft/2020-12/schema"}
	  }
	}`)
	out, err := SanitizeSchema(in)
	if err != nil {
		t.Fatalf("SanitizeSchema: %v", err)
	}
	if strings.Contains(string(out), "$schema") {
		t.Errorf("$schema survived sanitization: %s", out)
	}
	var doc map[string]any
	if err := json.Unmarshal(out, &doc); err != nil {
		t.Fatal(err)
	}
	props := doc["properties"].(map[string]any)
	if _, ok := props["q"]; !ok {
		t.Errorf("properties mangled: %s", out)
	}
}

// TestSanitizeSchemaRejectsExternalRef covers rule 3: any $ref pointing outside
// the document (http/https/file) is an error — offline compilation cannot
// resolve it and it is an SSRF surface. Local refs stay.
func TestSanitizeSchemaRejectsExternalRef(t *testing.T) {
	for _, ref := range []string{
		"http://evil.example/schema.json",
		"https://evil.example/schema.json",
		"file:///C:/Windows/system32/evil.json",
	} {
		in := json.RawMessage(`{"type":"object","properties":{"q":{"$ref":"` + ref + `"}}}`)
		if _, err := SanitizeSchema(in); err == nil {
			t.Errorf("external ref %q accepted", ref)
		} else if !strings.Contains(err.Error(), "$ref") {
			t.Errorf("ref %q error lacks context: %v", ref, err)
		}
	}
	// A local ref must survive untouched.
	local := json.RawMessage(`{"type":"object","definitions":{"s":{"type":"string"}},"properties":{"q":{"$ref":"#/definitions/s"}}}`)
	out, err := SanitizeSchema(local)
	if err != nil {
		t.Fatalf("local ref rejected: %v", err)
	}
	if !strings.Contains(string(out), "#/definitions/s") {
		t.Errorf("local ref dropped: %s", out)
	}
}

// TestSanitizeSchemaKeepsAdditionalProperties covers rule 4: a missing
// additionalProperties stays missing (never tightened) and an existing one is
// preserved verbatim.
func TestSanitizeSchemaKeepsAdditionalProperties(t *testing.T) {
	in := json.RawMessage(`{"type":"object","properties":{"a":{"type":"string"}}}`)
	out, err := SanitizeSchema(in)
	if err != nil {
		t.Fatalf("SanitizeSchema: %v", err)
	}
	if strings.Contains(string(out), "additionalProperties") {
		t.Errorf("additionalProperties was added: %s", out)
	}
	in2 := json.RawMessage(`{"type":"object","additionalProperties":true}`)
	out2, err := SanitizeSchema(in2)
	if err != nil {
		t.Fatalf("SanitizeSchema: %v", err)
	}
	if !strings.Contains(string(out2), `"additionalProperties":true`) {
		t.Errorf("existing additionalProperties lost: %s", out2)
	}
}

// TestSanitizeSchemaTopLevelObject covers rule 5: valid JSON whose top level
// is not an object is an error (drop the tool).
func TestSanitizeSchemaTopLevelObject(t *testing.T) {
	for _, in := range []json.RawMessage{
		json.RawMessage(`["array"]`),
		json.RawMessage(`"string"`),
		json.RawMessage(`42`),
		json.RawMessage(`true`),
	} {
		if _, err := SanitizeSchema(in); err == nil {
			t.Errorf("non-object top level %s accepted", in)
		}
	}
}

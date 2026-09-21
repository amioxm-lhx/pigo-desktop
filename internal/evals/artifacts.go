package evals

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// Artifact type constants for the pigo eval artifact registry.
const (
	SessionArtifactType = "pigo/evals:session"
	SourceArtifactType  = "pigo/evals:source"
)

// ArtifactAttachment is one persisted file inside an artifact.
type ArtifactAttachment struct {
	Name        string `json:"name"`
	ContentType string `json:"contentType"`
	Body        string `json:"body"`
}

// EvalArtifact is a run-scoped artifact reference.
type EvalArtifact struct {
	Type        string               `json:"type"`
	RunID       string               `json:"runId"`
	Attachments []ArtifactAttachment `json:"attachments"`
}

// ArtifactReference points to a persisted artifact relative to the artifact
// directory.
type ArtifactReference struct {
	Name string `json:"name"`
	Path string `json:"path"`
}

// PersistEvalArtifactReferences writes session and source artifacts for runID
// into artifactDirectory and returns their relative paths. An empty
// artifactDirectory is a no-op (returns nil), so callers can run evaluations
// without persistence. Attachment names must be plain base names — a path
// separator inside one is rejected rather than escaped, keeping the on-disk
// layout a flat two-level tree.
func PersistEvalArtifactReferences(artifacts []EvalArtifact, runID, artifactDirectory string) ([]ArtifactReference, error) {
	if artifactDirectory == "" {
		return nil, nil
	}
	// runID becomes one path segment under the artifact directory; reject any
	// value that is not a plain base name (separators, ".", "..") so a hostile
	// id cannot traverse out of the tree.
	if runID == "." || runID == ".." || strings.ContainsAny(runID, `/\`) {
		return nil, fmt.Errorf("invalid eval artifact run id: %s", runID)
	}
	references := make([]ArtifactReference, 0)

	for _, artifact := range artifacts {
		if (artifact.Type != SessionArtifactType && artifact.Type != SourceArtifactType) || artifact.RunID != runID {
			continue
		}
		category := "sources"
		if artifact.Type == SessionArtifactType {
			category = "sessions"
		}
		for _, attachment := range artifact.Attachments {
			name := filepath.Base(attachment.Name)
			if name != attachment.Name || name == "." || name == ".." {
				return nil, fmt.Errorf("invalid eval artifact name: %s", attachment.Name)
			}
			directory := filepath.Join(artifactDirectory, category, runID)
			if err := os.MkdirAll(directory, 0o700); err != nil {
				return nil, err
			}
			path := filepath.Join(directory, name)
			if err := os.WriteFile(path, []byte(attachment.Body), 0o600); err != nil {
				return nil, err
			}
			relative, err := filepath.Rel(artifactDirectory, path)
			if err != nil {
				return nil, err
			}
			references = append(references, ArtifactReference{Name: name, Path: filepath.ToSlash(relative)})
		}
	}
	return references, nil
}

// HarnessRunRecord is the JSONL schema written by the eval reporter: one line
// per completed harness run, appended to runs.jsonl beneath the artifact
// directory.
type HarnessRunRecord struct {
	SchemaVersion int                 `json:"schemaVersion"`
	RunID         string              `json:"runId"`
	Test          map[string]any      `json:"test"`
	Harness       string              `json:"harness"`
	Usage         map[string]any      `json:"usage,omitempty"`
	Timings       map[string]any      `json:"timings,omitempty"`
	Errors        []any               `json:"errors,omitempty"`
	Artifacts     []ArtifactReference `json:"artifacts"`
	Metadata      map[string]any      `json:"metadata,omitempty"`
}

// AppendHarnessRunReport appends one report record to runs.jsonl beneath the
// artifact directory. An empty directory is a no-op, mirroring
// PersistEvalArtifactReferences.
func AppendHarnessRunReport(artifactDirectory string, record HarnessRunRecord) error {
	if strings.TrimSpace(artifactDirectory) == "" {
		return nil
	}
	record.SchemaVersion = 1
	if err := os.MkdirAll(artifactDirectory, 0o700); err != nil {
		return err
	}
	encoded, err := json.Marshal(record)
	if err != nil {
		return err
	}
	file, err := os.OpenFile(filepath.Join(artifactDirectory, "runs.jsonl"), os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0o600)
	if err != nil {
		return err
	}
	defer file.Close()
	_, err = file.Write(append(encoded, '\n'))
	return err
}

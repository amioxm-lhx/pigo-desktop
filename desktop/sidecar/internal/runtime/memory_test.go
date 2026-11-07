package runtime

import (
	"os"
	"path/filepath"
	"testing"
)

// TestBuildMemoryReportCountsReconciledFiles proves /memory reports what is on
// disk: a Markdown memory file written after the index was created must appear
// in the counts, which is only true because the report reconciles first.
func TestBuildMemoryReportCountsReconciledFiles(t *testing.T) {
	root := t.TempDir()
	report := BuildMemoryReport(root)
	if !report.Enabled {
		t.Fatalf("a resolved root must report enabled: %+v", report)
	}
	if report.Error != "" {
		t.Fatalf("unexpected error: %s", report.Error)
	}
	if report.Total != 0 {
		t.Fatalf("empty root should report no entries, got %d", report.Total)
	}
	if report.DBPath != filepath.Join(root, "index.db") {
		t.Fatalf("index path = %q", report.DBPath)
	}

	dir := filepath.Join(root, "global", "user")
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("create scope dir: %v", err)
	}
	body := "---\nname: respond-chinese\ndescription: Reply in Chinese\nmetadata:\n  type: user\n---\n\nPrefers Chinese answers.\n"
	if err := os.WriteFile(filepath.Join(dir, "respond-chinese.md"), []byte(body), 0o644); err != nil {
		t.Fatalf("write memory file: %v", err)
	}

	report = BuildMemoryReport(root)
	if report.Error != "" {
		t.Fatalf("unexpected error after reconcile: %s", report.Error)
	}
	if report.Counts["global"] != 1 || report.Total != 1 {
		t.Fatalf("expected one global entry, got counts=%v total=%d", report.Counts, report.Total)
	}
	if report.Indexed != 1 {
		t.Fatalf("expected the reconcile to index one file, got %d", report.Indexed)
	}

	// A second report is a no-op, so the card does not imply churn on refresh.
	again := BuildMemoryReport(root)
	if again.Total != 1 || again.Indexed != 0 {
		t.Fatalf("second report should be stable: total=%d indexed=%d", again.Total, again.Indexed)
	}
}

// TestBuildMemoryReportSurfacesOpenFailure keeps /memory a diagnostic: an
// unopenable index is reported in the payload instead of failing the request.
func TestBuildMemoryReportSurfacesOpenFailure(t *testing.T) {
	root := filepath.Join(t.TempDir(), "blocked")
	// A regular file where the root directory should be makes MkdirAll fail.
	if err := os.WriteFile(root, []byte("not a directory"), 0o644); err != nil {
		t.Fatalf("prepare blocker: %v", err)
	}
	report := BuildMemoryReport(root)
	if report.Error == "" {
		t.Fatal("expected an error for an unopenable memory root")
	}
	if report.Total != 0 {
		t.Fatalf("failed report must not invent counts: %+v", report)
	}
}

func TestMemoryRootDirFollowsPigoHome(t *testing.T) {
	home := t.TempDir()
	t.Setenv("PIGO_HOME", home)
	if got := MemoryRootDir(); got != filepath.Join(home, "memory") {
		t.Fatalf("memory root = %q", got)
	}
}

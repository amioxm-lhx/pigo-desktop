package evals

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math"
	"reflect"
	"strings"
)

// EvalHarnessIterationArtifact is the metadata attached to one harness run.
// It mirrors the schema-v1 artifact shape of the pi eval registry so reports
// stay comparable across harness generations.
type EvalHarnessIterationArtifact struct {
	SchemaVersion int      `json:"schemaVersion"`
	EvalSet       string   `json:"evalSet"`
	GroupKey      string   `json:"groupKey"`
	Harness       string   `json:"harness"`
	Baseline      string   `json:"baseline"`
	Candidates    []string `json:"candidates"`
	Repetition    int      `json:"repetition"`
}

// ParseEvalHarnessIterationArtifact validates an arbitrary JSON value and
// returns the artifact. The boolean is false when the value is not a valid
// schema-v1 artifact, so a caller can reject foreign metadata without
// treating it as a hard failure.
func ParseEvalHarnessIterationArtifact(value any) (EvalHarnessIterationArtifact, bool) {
	var artifact EvalHarnessIterationArtifact
	mapping, ok := asStringMap(value)
	if !ok {
		return artifact, false
	}

	if !jsonNumberIsInt(mapping["schemaVersion"], 1) {
		return artifact, false
	}
	artifact.SchemaVersion = 1
	artifact.EvalSet, ok = asString(mapping["evalSet"])
	if !ok {
		return artifact, false
	}
	artifact.GroupKey, ok = asString(mapping["groupKey"])
	if !ok {
		return artifact, false
	}
	artifact.Harness, ok = asString(mapping["harness"])
	if !ok {
		return artifact, false
	}
	artifact.Baseline, ok = asString(mapping["baseline"])
	if !ok {
		return artifact, false
	}
	artifact.Candidates, ok = asStringSlice(mapping["candidates"])
	if !ok {
		return artifact, false
	}
	artifact.Repetition, ok = asInt(mapping["repetition"])
	if !ok {
		return artifact, false
	}
	return artifact, true
}

// DecodeEvalHarnessIterationArtifact parses a JSON-encoded artifact and
// validates it against the schema-v1 invariants.
func DecodeEvalHarnessIterationArtifact(data []byte) (EvalHarnessIterationArtifact, error) {
	var artifact EvalHarnessIterationArtifact
	if err := json.Unmarshal(data, &artifact); err != nil {
		return EvalHarnessIterationArtifact{}, err
	}
	if artifact.SchemaVersion != 1 || artifact.EvalSet == "" || artifact.GroupKey == "" ||
		artifact.Harness == "" || artifact.Baseline == "" || len(artifact.Candidates) == 0 ||
		artifact.Repetition < 1 {
		return EvalHarnessIterationArtifact{}, fmt.Errorf("invalid eval harness iteration artifact")
	}
	return artifact, nil
}

// CanonicalizeJSON returns the stable JSON encoding of value with object keys
// sorted. It rejects values that cannot be represented as finite JSON,
// including circular data and non-plain-object shapes (structs), so two runs
// that differ only in map iteration order hash identically.
func CanonicalizeJSON(value any) ([]byte, error) {
	normalized, err := canonicalizeValue(value, make(map[visit]bool))
	if err != nil {
		return nil, err
	}
	return json.Marshal(normalized)
}

// HashInput derives the input key used to group repeated eval inputs. When
// the input is a plain object carrying a non-empty "id" string, the id is the
// key (the fast path for task-set entries); otherwise the key is the SHA-256
// of the canonical encoding.
func HashInput(input any) (string, error) {
	if mapping, ok := asStringMap(input); ok {
		if id, exists := mapping["id"]; exists {
			if text, ok := id.(string); ok {
				text = strings.TrimSpace(text)
				if text != "" {
					return text, nil
				}
			}
		}
	}

	canonical, err := CanonicalizeJSON(input)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(canonical)
	return hex.EncodeToString(sum[:]), nil
}

// DeriveEvalGroupKey returns the JSON array [inputKey, repetition] encoded as
// a string, matching the pi group key shape so grouping is stable across
// harness generations.
func DeriveEvalGroupKey(input any, repetition int) (string, error) {
	inputKey, err := HashInput(input)
	if err != nil {
		return "", err
	}
	encoded, err := json.Marshal([]any{inputKey, repetition})
	if err != nil {
		return "", err
	}
	return string(encoded), nil
}

// visit identifies one composite value already being canonicalized, so
// circular references are detected instead of recursing forever.
type visit struct {
	kind reflect.Kind
	ptr  uintptr
}

func canonicalizeValue(value any, ancestors map[visit]bool) (any, error) {
	if value == nil {
		return nil, nil
	}
	switch typed := value.(type) {
	case string:
		return typed, nil
	case bool:
		return typed, nil
	case json.Number:
		number, err := typed.Float64()
		if err != nil || math.IsNaN(number) || math.IsInf(number, 0) {
			return nil, fmt.Errorf("eval input must contain only finite numbers")
		}
		return number, nil
	case float64:
		if math.IsNaN(typed) || math.IsInf(typed, 0) {
			return nil, fmt.Errorf("eval input must contain only finite numbers")
		}
		return typed, nil
	case float32:
		number := float64(typed)
		if math.IsNaN(number) || math.IsInf(number, 0) {
			return nil, fmt.Errorf("eval input must contain only finite numbers")
		}
		return number, nil
	case int:
		return float64(typed), nil
	case int8:
		return float64(typed), nil
	case int16:
		return float64(typed), nil
	case int32:
		return float64(typed), nil
	case int64:
		return float64(typed), nil
	case uint:
		return float64(typed), nil
	case uint8:
		return float64(typed), nil
	case uint16:
		return float64(typed), nil
	case uint32:
		return float64(typed), nil
	case uint64:
		return float64(typed), nil
	}

	reflected := reflect.ValueOf(value)
	switch reflected.Kind() {
	case reflect.Interface:
		if reflected.IsNil() {
			return nil, nil
		}
		return canonicalizeValue(reflected.Elem().Interface(), ancestors)
	case reflect.Ptr:
		if reflected.IsNil() {
			return nil, nil
		}
		key := visit{kind: reflect.Ptr, ptr: reflected.Pointer()}
		if ancestors[key] {
			return nil, fmt.Errorf("eval input must not contain circular references")
		}
		ancestors[key] = true
		defer delete(ancestors, key)
		return canonicalizeValue(reflected.Elem().Interface(), ancestors)
	case reflect.Array:
		result := make([]any, reflected.Len())
		for index := 0; index < reflected.Len(); index++ {
			item, err := canonicalizeValue(reflected.Index(index).Interface(), ancestors)
			if err != nil {
				return nil, err
			}
			result[index] = item
		}
		return result, nil
	case reflect.Slice:
		key := visit{kind: reflect.Slice, ptr: reflected.Pointer()}
		if ancestors[key] {
			return nil, fmt.Errorf("eval input must not contain circular references")
		}
		ancestors[key] = true
		defer delete(ancestors, key)
		result := make([]any, reflected.Len())
		for index := 0; index < reflected.Len(); index++ {
			item, err := canonicalizeValue(reflected.Index(index).Interface(), ancestors)
			if err != nil {
				return nil, err
			}
			result[index] = item
		}
		return result, nil
	case reflect.Map:
		if reflected.Type().Key().Kind() != reflect.String {
			return nil, fmt.Errorf("eval input objects must have string keys")
		}
		key := visit{kind: reflect.Map, ptr: reflected.Pointer()}
		if ancestors[key] {
			return nil, fmt.Errorf("eval input must not contain circular references")
		}
		ancestors[key] = true
		defer delete(ancestors, key)
		result := make(map[string]any, reflected.Len())
		iter := reflected.MapRange()
		for iter.Next() {
			item, err := canonicalizeValue(iter.Value().Interface(), ancestors)
			if err != nil {
				return nil, err
			}
			result[iter.Key().String()] = item
		}
		return result, nil
	case reflect.Struct:
		return nil, fmt.Errorf("eval input must contain only plain objects and arrays")
	default:
		return nil, fmt.Errorf("eval input must be JSON-serializable")
	}
}

func asStringMap(value any) (map[string]any, bool) {
	if value == nil {
		return nil, false
	}
	reflected := reflect.ValueOf(value)
	if reflected.Kind() == reflect.Ptr || reflected.Kind() == reflect.Interface {
		if reflected.IsNil() {
			return nil, false
		}
		return asStringMap(reflected.Elem().Interface())
	}
	if reflected.Kind() != reflect.Map || reflected.Type().Key().Kind() != reflect.String {
		return nil, false
	}
	result := make(map[string]any, reflected.Len())
	iter := reflected.MapRange()
	for iter.Next() {
		result[iter.Key().String()] = iter.Value().Interface()
	}
	return result, true
}

func asString(value any) (string, bool) {
	text, ok := value.(string)
	return text, ok
}

func asStringSlice(value any) ([]string, bool) {
	reflected := reflect.ValueOf(value)
	if reflected.Kind() == reflect.Ptr || reflected.Kind() == reflect.Interface {
		if reflected.IsNil() {
			return nil, false
		}
		return asStringSlice(reflected.Elem().Interface())
	}
	if reflected.Kind() != reflect.Array && reflected.Kind() != reflect.Slice {
		return nil, false
	}
	result := make([]string, reflected.Len())
	for index := 0; index < reflected.Len(); index++ {
		text, ok := reflected.Index(index).Interface().(string)
		if !ok {
			return nil, false
		}
		result[index] = text
	}
	return result, true
}

func asInt(value any) (int, bool) {
	switch typed := value.(type) {
	case int:
		return typed, true
	case int8:
		return int(typed), true
	case int16:
		return int(typed), true
	case int32:
		return int(typed), true
	case int64:
		return int(typed), true
	case uint:
		return int(typed), true
	case uint8:
		return int(typed), true
	case uint16:
		return int(typed), true
	case uint32:
		return int(typed), true
	case uint64:
		return int(typed), true
	case float64:
		if typed == math.Trunc(typed) {
			return int(typed), true
		}
	}
	return 0, false
}

func jsonNumberIsInt(value any, expected int) bool {
	number, ok := asInt(value)
	return ok && number == expected
}

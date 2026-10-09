package main

import (
	"strings"
	"testing"
)

const fixMsgX = "fix: x"

func TestParseConventionalCommit_TableDriven(t *testing.T) {
	t.Parallel()

	type want struct {
		errContains string
		kind        string
		scope       string
		hash        string
		description string
		body        string
		refersTo    []string
		breaking    bool
	}
	tests := []struct {
		name         string
		message      string
		hasHash      bool
		allowedKinds []string
		want         want
	}{
		{
			name:         "leading whitespace",
			message:      " leading: desc",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "commit message line number 1 (` leading: desc`) should not have have leading or trailing whitespace"},
		},
		{
			name:         "trailing whitespace",
			message:      "trailing: desc ",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "trailing whitespace"},
		},
		{
			name:         "empty message",
			message:      "",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "empty commit message"},
		},
		{
			name:         "empty header",
			message:      "\nbody",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "empty commit message header"},
		},
		{
			name:         "valid hash prefix",
			message:      "0123456 feat: add stuff",
			hasHash:      true,
			allowedKinds: []string{CommitKindFeat},
			want:         want{hash: "0123456", kind: CommitKindFeat, description: "add stuff"},
		},
		{
			name:         "invalid hash content",
			message:      "ZZZZZZZ feat: x",
			hasHash:      true,
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "lowercase hexadecimal"},
		},
		{
			name:         "missing space after hash",
			message:      "0123456feat: x",
			hasHash:      true,
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "commit hash length must be either 7 or 40 characters long"},
		},
		{
			name:         "kind scope bang",
			message:      "feat(scope)!: do something",
			allowedKinds: []string{CommitKindFeat},
			want:         want{kind: CommitKindFeat, scope: "scope", breaking: true, description: "do something"},
		},
		{
			name:         "missing colon",
			message:      "feat scope do",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "must have colon"},
		},
		{
			name:         "space before paren",
			message:      "feat (scope): x",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "spaces before `('"},
		},
		{
			name:         "empty scope",
			message:      "feat(): x",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "scope cannot be empty"},
		},
		{
			name:         "no space after colon",
			message:      "feat:do",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "space after colon"},
		},
		{
			name:         "description ends with period",
			message:      "feat: finishes.",
			allowedKinds: []string{CommitKindFeat},
			want:         want{errContains: "should not end with period"},
		},
		{
			name:         "kind not allowed",
			message:      "chore: x",
			allowedKinds: []string{CommitKindFeat, CommitKindFix},
			want:         want{errContains: "not allowed"},
		},
		{
			name: "body and refs",
			message: strings.Join([]string{
				"fix: something broke",
				"",
				"this is the body line 1",
				"line 2",
				"",
				"Refs: 0123456, 89abcde",
			}, "\n"),
			allowedKinds: []string{CommitKindFix},
			want:         want{body: "this is the body line 1\nline 2", refersTo: []string{"0123456", "89abcde"}},
		},
		{
			name: "colons without space in body prose",
			message: strings.Join([]string{
				"fix: stop the forms plugin from drawing a blue focus ring",
				"",
				"the browser renders: a white background, and a blue.600 box-shadow",
				"publish:dry",
				"focus ring that contradicts *:focus { outline: none }",
			}, "\n"),
			allowedKinds: []string{CommitKindFix},
			want: want{
				body: strings.Join([]string{
					"the browser renders: a white background, and a blue.600 box-shadow",
					"publish:dry",
					"focus ring that contradicts *:focus { outline: none }",
				}, "\n"),
			},
		},
		{
			name: "non refs colon line is ignored",
			message: strings.Join([]string{
				"fix: something broke",
				"",
				"Signed-off-by:someone",
			}, "\n"),
			allowedKinds: []string{CommitKindFix},
			want:         want{body: "Signed-off-by:someone"},
		},
		{
			name: "breaking in footer",
			message: strings.Join([]string{
				"feat: add x",
				"",
				"Notes",
				"",
				"BREAKING CHANGE: API changed",
			}, "\n"),
			allowedKinds: []string{CommitKindFeat},
			want:         want{breaking: true},
		},
		{
			name:         "refs wrong case",
			message:      strings.Join([]string{fixMsgX, "", "refs: 0123456"}, "\n"),
			allowedKinds: []string{CommitKindFix},
			want:         want{errContains: "consistent case"},
		},
		{
			name:         "bad hash in refs",
			message:      strings.Join([]string{fixMsgX, "", "Refs: badhash"}, "\n"),
			allowedKinds: []string{CommitKindFix},
			want:         want{errContains: "invalid hash"},
		},
		{
			name:         "no space after colon in end",
			message:      strings.Join([]string{fixMsgX, "", "Refs:0123456"}, "\n"),
			allowedKinds: []string{CommitKindFix},
			want:         want{errContains: "one space after colon"},
		},
		{
			name:         "comma spacing in refs",
			message:      strings.Join([]string{fixMsgX, "", "Refs: 0123456,89abcdef"}, "\n"),
			allowedKinds: []string{CommitKindFix},
			want:         want{errContains: "delimited by comma and a single space"},
		},
		{
			name:         "revert requires refs",
			message:      "revert: undo",
			allowedKinds: []string{"revert"},
			want:         want{errContains: "revert commits must have hashes of the commits they revert; add `Refs: ` attribute to commit footer"},
		},
		{
			name:         "valid revert with refs",
			message:      strings.Join([]string{"revert: undo", "", "Refs: 0123456"}, "\n"),
			allowedKinds: []string{"revert"},
			want:         want{refersTo: []string{"0123456"}},
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			c, err := ParseConventionalCommit(tc.message, tc.hasHash, tc.allowedKinds)
			if tc.want.errContains != "" {
				if err == nil || !strings.Contains(err.Error(), tc.want.errContains) {
					t.Fatalf("expected error containing %q, got err=%v, commit=%+v", tc.want.errContains, err, c)
				}
				return
			}
			if err != nil {
				t.Fatalf("unexpected error: %v", err)
			}
			if tc.want.kind != "" && c.Kind != tc.want.kind {
				t.Fatalf("kind: want %q got %q", tc.want.kind, c.Kind)
			}
			if tc.want.scope != "" && c.Scope != tc.want.scope {
				t.Fatalf("scope: want %q got %q", tc.want.scope, c.Scope)
			}
			if tc.want.hash != "" && c.Hash != tc.want.hash {
				t.Fatalf("hash: want %q got %q", tc.want.hash, c.Hash)
			}
			if tc.want.description != "" && c.Description != tc.want.description {
				t.Fatalf("description: want %q got %q", tc.want.description, c.Description)
			}
			if tc.want.body != "" && c.Body != tc.want.body {
				t.Fatalf("body: want %q got %q", tc.want.body, c.Body)
			}
			if tc.want.breaking != c.Breaking {
				t.Fatalf("breaking: want %v got %v", tc.want.breaking, c.Breaking)
			}
			if len(tc.want.refersTo) > 0 {
				if len(c.RefersTo) != len(tc.want.refersTo) {
					t.Fatalf("refersTo length: want %d got %d", len(tc.want.refersTo), len(c.RefersTo))
				}
				for i := range tc.want.refersTo {
					if c.RefersTo[i] != tc.want.refersTo[i] {
						t.Fatalf("refersTo[%d]: want %q got %q", i, tc.want.refersTo[i], c.RefersTo[i])
					}
				}
			}
		})
	}
}

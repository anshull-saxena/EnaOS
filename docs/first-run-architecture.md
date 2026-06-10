# First-Run Experience — EnaOS

> **Status:** Accurate as of v0.1.0-developer-preview
> Describes the actual first-run onboarding implementation.

## Architecture Overview

The first-run experience spans two subsystems:

```
enad (daemon)
  └── FirstRunState         — tracks onboarding progress, manages demo data
  └── SuggestionEngine      — enhanced with onboarding-specific rules
  └── SnapshotStore         — seeded demo snapshot for fresh installs

ena-bar (GTK4 shell)
  └── WelcomeOverlay        — subtle intro overlay widget
  └── Bar                   — enhanced with first-run state awareness
  └── AmbientSuggestion     — enhanced for progressive discovery hints
  └── RestorationWidget     — demo restoration moment
```

## Onboarding Interaction Model

**Phase 0: Fresh Install Detection**
- enad checks for existing snapshot/memory DB on first launch
- No DB → fresh install → set `FirstRunState::Fresh`
- Seed demo snapshot + example orchestration history

**Phase 1: Welcome Overlay (0-15s)**
- Appears on first bar launch after enad connects
- Subtle crossfade reveal (not a modal, not a tutorial)
- Contains:
  - EnaOS wordmark + tagline
  - Three contextual suggestion chips (anchored to real capabilities)
  - "Continue to EnaOS" subtle button
- Dismisses on: click anywhere, Escape, first command submission, or 12s timeout
- Never shows again after dismissal

**Phase 2: Guided Discovery (15-120s)**
- Ambient suggestions shift to onboarding mode:
  1. "Try typing a command — your environment is ready" (after first expand)
  2. "Type 'open' to find apps" (if user stares at empty input)
  3. "Your workspace snapshots are saved automatically" (after first snapshot)
  4. "Type '?' for keyboard shortcuts" (after 3rd command)
- Each hint dismisses after 8s or on next user action
- Max 4 hints total — no fatigue

**Phase 3: First Restoration Moment**
- After 2nd session (bar reconnects after enad restart):
  - Show restoration suggestion with seeded demo data
  - "Continue: EnaOS Development · 3 windows, 2 terminals"
  - User gets to experience restoration on real (demo) data
- This demonstrates continuity naturally

**Phase 4: Operational Confidence**
- After first successful orchestration plan:
  - Status: "✓ Plan completed — 3/3 steps"
  - Timeline widget shows execution visibility
- After first command execution:
  - Result revealer shows output
- After first system awareness event:
  - Context label shows battery, network, focused app

## Demo-State Strategy

For fresh installs, enad seeds:

1. **Demo Snapshot** — labeled "EnaOS Development" with:
   - 3 windows (code editor, terminal, browser)
   - 2 terminal sessions
   - Active project: "EnaOS"
   - Created: "2 hours ago"

2. **Demo Orchestration Plan** (completed):
   - Title: "Setup development environment"
   - Nodes: "Open editor" ✓, "Start dev server" ✓, "Open docs" ✓

3. **Demo Suggestions** (expired, show as examples):
   - "Continue: EnaOS Development"
   - "Good morning — ready to work?"

All demo data is clearly marked as `demo: true` in metadata.
Demo data auto-expires after first real snapshot is taken.

## Welcome Overlay — GTK UX Hierarchy

```
gtk4::Revealer (crossfade, 400ms)
  └── gtk4::Box (vertical, centered)
       ├── gtk4::Label (wordmark or icon)
       ├── gtk4::Label (tagline: "Your AI-native environment")
       ├── gtk4::Box (horizontal, suggestion chips)
       │    ├── gtk4::Button ("Try asking a question")
       │    ├── gtk4::Button ("Open an app")
       │    └── gtk4::Button ("Check system status")
       └── gtk4::Label ("Press Escape or click to dismiss")
```

Styling: Dark glass background, subtle border, 14px border-radius.
Font: Inter, 300 weight for tagline, 450 for chips.
No shadows — feels like part of the OS, not a web popup.

## Contextual Hint System

The existing `SuggestionEngine` in enad is enhanced with onboarding rules:

```
onboarding_rules:
  - first_expand: "Try typing a command to see context suggestions"
  - first_empty_input: "Type a few letters to find commands"
  - after_first_command: "Commands run through enad — your secure daemon"
  - after_first_snapshot: "Workspace snapshots save your environment automatically"
  - after_second_session: "Welcome back — your workspace was restored"
```

These are emitted as standard `SuggestionGenerated` events, but with `priority >= 0.75`
and a special `kind: "onboarding"` tag. The bar filters these differently:
- Never shown while welcome overlay is active
- Shown with longer auto-dismiss duration (12s vs 8s)
- Max 4 shown per lifetime

## Dismissal + Rediscovery Model

**Dismissal:**
- Welcome overlay: 12s auto-dismiss OR any click/keyboard action
- Onboarding hints: 8-12s auto-dismiss OR next user action
- Restoration suggestion: user Dismiss button OR auto-dismiss after restore completes
- All dismissals are permanent for the current session

**Rediscovery:**
- Welcome overlay: NEVER shows again (`first_run_completed: true` in memory)
- Onboarding hints: if user dismisses all and shows no engagement for 60s,
  a single subtle hint reappears: "Ena is ready when you are"
- Restoration: shows every session until user restores or explicitly dismisses 3x
- System features (orchestration, context, etc.): discovered naturally through use, not hints

## Empty/Loading/Error States

**No snapshots yet:**
```
"Saving your first workspace snapshot... Auto-snapshots are enabled."
```
Shown in restoration widget area for first-time users.

**No suggestions yet:**
```
"Type a command or use Ctrl+Space to activate Ena."
```
Shown in ambient area when empty.

**Daemon disconnected:**
```
Status dot: grey
Label: "enad: disconnected — reconnecting..."
```
Already implemented.

**Loading preview:**
```
Skeleton: pulsing grey bar in place of preview content
Button: disabled, label "Loading..."
```
Already partially implemented.

**Error state:**
```
Action bar: "✗ {error}" with auto-dismiss after 5s
```
Already implemented.

## Smooth GTK-Native Motion

- Welcome overlay: 400ms crossfade (revealer)
- Suggestion chips: staggered reveal (each chip 100ms after previous)
- All transitions: elegant cubic-bezier curves
- Dismissal: 300ms crossfade
- No spring animations, no JavaScript-like effects — pure GTK transitions

## Implementation Order

1. ✅ Read and understand existing codebase
2. FirstRunState in enad (first_run.rs)
3. Seeded demo data in enad
4. Welcome overlay widget in ena-bar (welcome_overlay.rs)
5. Wire welcome overlay into bar lifecycle
6. Onboarding ambient suggestions in suggestion engine
7. CSS styles for welcome overlay
8. Dismissal + rediscovery logic
9. Build and verify
10. Code review

---

*End of architecture document.*

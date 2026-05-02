# Gemini Entry Instructions (Public Bootstrap)

This file is a small public bootstrap for Gemini-class agents working in `d2r-core`.
It must stay concise and publishable. Detailed private workflow, indexing, skill discovery, and task-routing rules belong in the Strategy Hub resolved from `D2R_SPEC_PATH`.

## Canonical References
Read these in order:
1. `AGENTS.md` (public safety floor)
2. `NAVIGATOR.md` (public implementation and verification index)
3. If a local Strategy Hub is available, prefer its model-specific authority:
   - `<D2R_SPEC_PATH>/GEMINI.md`
   - `<D2R_SPEC_PATH>/AGENTS.md`
   - `<D2R_SPEC_PATH>/AI_WORKFLOW.md`
   - relevant `<D2R_SPEC_PATH>/.agents/tasks/*.md` or `discussion/*.md`

If the Strategy Hub is absent or inaccessible, continue with only the public root documents and source tree. Do not block normal core usage on private overlay material.

## Strategy Hub Shape
When available, the Strategy Hub is expected to provide the private operating context that this public bootstrap intentionally does not duplicate:
- model entrypoints such as `GEMINI.md` or `CLAUDE.md`;
- `NAVIGATOR.md` for private domain routing;
- `AGENTS.md` and `AI_WORKFLOW.md` for workflow and publication-boundary rules;
- `.agents/tasks/` for parent tasks and mini-spec handoffs;
- `discussion/`, `research/`, or local navigation/cache files for deeper provenance and recent-delta context.

Core-side agents should use those files when present, but should not copy private detail back into public core documents.

## Role
Act as a strategic research, analysis, and planning model.
Use this core-side file only to enter the public project safely; use the spec-side `GEMINI.md` for detailed Gemini workflow when available.

For simple, well-bounded execution or review requests with clear scope, respond directly. For complex work, bit-level ambiguity, multi-file planning, or handoff preparation, route planning through the Strategy Hub when available.

## Core Work Must-Do Floor
Use this section when working on `d2r-core` from a wider `d2r` workspace where Strategy Hub instructions may not be automatically loaded.
- **Context Efficiency & Custom Tool Priority**: NEVER use generic shell commands like `cat`, `grep`, `sed`, or `ls` via `run_shell_command`. You MUST prioritize custom system tools (`read_file`, `grep_search`, `replace`, `list_directory`) to minimize context token usage.
- If Gemini CLI was launched from the top-level `d2r` workspace, first follow the workspace `GEMINI.md` (tracked source: `d2r-spec/.gemini/UPPER-ROOT-GEMINI.md`).
- Resolve the Strategy Hub through `D2R_SPEC_PATH` first; if unset, check the sibling `../d2r-spec/` path.
- Before non-trivial core edits, read the relevant Strategy Hub entrypoint and task context when available: `GEMINI.md`, `AGENTS.md`, `AI_WORKFLOW.md`, and active `.agents/tasks/*.md`.
- For quick recent-delta context, prefer `<D2R_SPEC_PATH>/.agents/navigation/active-context.md` or `d2map-query context` when those tools/files are available; do not duplicate detailed indexing rules here.
- Use verifier JSON output (`--json`) when supported, and trust structured `hints`/`metadata` over guesses.
- On Windows/Gemini CLI, follow the workspace `GEMINI.md` `Gemini CLI Encoding Floor` to prevent terminal mojibake:
  - **Mandatory Preamble**: Prefix ALL `run_shell_command` calls with:
    ```powershell
    $OutputEncoding = [Console]::InputEncoding = [Console]::OutputEncoding = [System.Text.Encoding]::UTF8;
    ```
  - **Preferred Tool**: Use the Strategy Hub native helper for complex execution or Korean text: `..\d2r-spec\.agents\tools\d2r-agent-helper\target\debug\d2r-agent-helper.exe exec -- <command>`
  - **Argument Protection**: For commands with complex Korean arguments, use PowerShell `-EncodedCommand` (UTF-16LE + Base64) to prevent shell-level corruption.
  - **Mojibake Gate**: Stop immediately if output shows `??`, replacement blocks, or unreadable Korean. Re-run through the helper or UTF-8 preamble.

- If the work spans 3+ files, reopens bit-level ambiguity, or needs private reasoning, stop for Strategy Hub planning instead of broad direct implementation.
- If the Strategy Hub is unavailable and the task depends on private fixture/research truth, stop and report the missing context rather than inventing assumptions.

## Public Safety Rules
- Never execute `git push` without an explicit user command.
- Preserve repository boundaries: `d2r-core` contains public implementation logic; extracted game data belongs in `d2r-data`; private reasoning belongs in `d2r-spec`.
- Keep public docs free of private reverse-engineering detail. Link or summarize only public-safe conclusions.
- Prefer verifier-backed conclusions over guesses. Use public verification tools documented in `NAVIGATOR.md`.
- If Strategy Hub guidance conflicts with public safety constraints, keep the public safety constraints intact.

## Directive Updates
Before modifying directive files such as `AGENTS.md`, `GEMINI.md`, `CLAUDE.md`, or `CONSTITUTION.md`, follow the public `AGENTS.md` directive-update protocol: conflict check, action plan, side-effect scan, then the smallest safe edit.

# Framework gaps

Places where Div (pinned in `Cargo.toml`) cannot do what motion needs. Phase
1 found the gaps in [design.md](design.md#works--workaround--missing), where
each has a number `M<n>`, an owner and a size. Phase 2 adds an entry here
for each gap the gallery works around: what was tried, what happened, the
workaround, and how bad it is.

Each workaround in the code carries a `div-gap: M<n>` comment, so it can be
found and removed when Div fixes the gap: `git grep -n "div-gap: M12"`.

Pinned Div: `565db46a`.

| # | Gap | Area | Severity | Status |
| --- | --- | --- | --- | --- |

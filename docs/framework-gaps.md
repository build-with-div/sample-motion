# Framework gaps

The web platform features Div is missing for the motion packages are in
[design.md](design.md#platform-features-what-div-has-what-is-missing). Each
has a number `F<n>`, an owner and a size. Div should implement those
features. This repository's ports use only Div's public surface.

A workaround here is temporary scaffolding that unblocks building a port.
It is never the proposed solution. Each one:

- carries a `div-gap: F<n>` comment naming the platform feature that will
  replace it, so `git grep -n "div-gap: F4"` finds every use;
- has an entry below: what the package needs, what was tried, what
  happened, the scaffold used, and how bad it is;
- is deleted when Div ships the feature.

Pinned Div: `565db46a`.

| # | Scaffold | Replaced by | Package | Status |
| --- | --- | --- | --- | --- |

# Contributing

Coding agents run the commands in the "Build and test" section of
[AGENTS.md](AGENTS.md). That section is pinned in `.mdatron/pins.yaml`, so a
change to it fails CI until a maintainer has read the new commands and
re-pinned them with `mdatron pin --update`.

mdatron also checks that every `AGENTS.md` keeps its required sections and
working links, and that no `AGENTS.override.md` is committed; see
`.mdatron/routes.yaml`.

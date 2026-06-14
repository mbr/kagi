# kagi

`kagi` is a command-line client for the Kagi Search and Extract APIs.

It is designed for interactive shell use and for coding agents that should use ordinary CLI tools instead of an MCP server. Output defaults to markdown for readability, and `--format json` returns raw Kagi API responses for piping into tools like `jq`.

## Authentication

Set `KAGI_API_KEY` in the environment:

```sh
export KAGI_API_KEY=...
```

Alternatively, write the key to the user configuration directory:

```sh
mkdir -p ~/.config/kagi
printf '%s\n' '...' > ~/.config/kagi/api-key
chmod 600 ~/.config/kagi/api-key
```

You can also pass a key directly with `--api-key`.

## Search

Search is invoked through the `search` subcommand:

```sh
kagi search 'rust tokio graceful shutdown' --limit 5
```

Markdown is the default output format. Use `--format json` for raw API JSON:

```sh
kagi search 'rust tokio graceful shutdown' --limit 5 --format json | jq '.data.search[] | {title, url}'
```

Useful search options include:

```sh
kagi search 'query' --workflow news
kagi search 'query' --page 2 --limit 10
kagi search 'query' --filters.region DE --filters.after 2026-01-01
kagi search 'query' --lens.sites_included docs.rs --lens.sites_excluded reddit.com
kagi search 'query' --extract.count 3
```

For less common or newly added API fields, merge raw JSON into the request body:

```sh
kagi search 'query' --request-json '{"safe_search":false}'
```

## Extract

Extract markdown from up to ten HTTPS URLs:

```sh
kagi extract https://example.com/a https://example.com/b
```

Use `--format json` for raw API JSON:

```sh
kagi extract https://kagi.com/api/docs/openapi.md --format json | jq '.data[0].markdown'
```

## Nix

The flake exposes the CLI as `packages.default` and the Pi prompt extension as `packages.piExtension`.

For Home Manager, import `homeManagerModules.default` and enable:

```nix
programs.kagi = {
  enable = true;
  piExtension.enable = true;
};
```

The Pi extension only teaches agents how to use the local `kagi` CLI; it does not configure authentication.

## License

Licensed under either of `Apache-2.0` or `MIT`, at your option.

## Development

Enter the development environment through `direnv` or `nix develop`, then run:

```sh
./check.sh
./format.sh
```

Build a release binary with:

```sh
cargo build --release
```

For use by `pi`, copy the release binary to `~/.pi/agent/bin/kagi`.

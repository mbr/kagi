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

## Ask

Extract pages and answer questions through an OpenAI-compatible Chat Completions
API. Configure the endpoint and its model in the environment; there is no
`--model` flag. For example:

```sh
export OPENAI_BASE_URL=https://api.openai.com/v1
export OPENAI_MODEL=your-model
export OPENAI_API_KEY=your-api-key

kagi ask https://example.com/report.pdf 'What is the reported revenue?'
```

`OPENAI_BASE_URL` defaults to `https://api.openai.com/v1`. `OPENAI_MODEL` is
required. Set `OPENAI_API_KEY` for providers that require authentication; it is
independent of `KAGI_API_KEY`.

The first positional argument is the page, the remaining words form the
question. The model is instructed to answer only from the sources, cite source
identifiers such as `[1]`, include supporting quotes, and say when the sources
do not contain the answer. These are instructions, not guarantees of factual
accuracy or protection against prompt injection. The model receives no tools,
local files, or conversation history.

Pull in additional pages as context, up to the extraction limit of ten:

```sh
kagi ask https://example.com/a 'Where do these disagree?' --url https://example.com/b
```

Answers are not verifiable on their own. Keep the extracted markdown to check
them against:

```sh
kagi ask https://example.com/spec 'Summarise the wire format' \
  --save-source spec.md
```

Every answer includes a numbered source URL list generated from extraction
metadata, so citations can be followed without `--save-source`.
Saved Markdown contains the same labeled sources sent to the model. All
requested pages must extract successfully and contain nonempty text before a
chat request is sent. Source content and your question are sent to the configured
chat provider; avoid sending private data to an endpoint you do not trust.

Chat requests have a 120-second deadline and are not retried by this client
(the proxy or provider may retry internally). The combined source and question
limit is 1 MiB of UTF-8 text, not a model-specific token count; nothing is silently
truncated. The completion budget is 8,192 tokens, including reasoning where
applicable. Incomplete, refused, empty, or tool-call responses are errors.
`--timeout` continues to control extraction only.

## Nix

The flake exposes the CLI as `packages.default` and the Pi prompt extension as `piExtensions.default`.

For Home Manager, import `homeManagerModules.default` and enable:

```nix
programs.kagi = {
  enable = true;
  enablePiExtension = true;
  apiKeyFile = "/run/secrets/kagi-api-key";
};
```

Use either `apiKeyFile` or `apiKey` to install `~/.config/kagi/api-key`; the options are mutually exclusive. `apiKey` stores the token in the Nix store, so prefer `apiKeyFile` for managed secrets.

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

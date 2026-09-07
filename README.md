# kagi

`kagi` is a client for the Kagi Search and Extract APIs, with a Perplexity-compatible HTTP search server for LiteLLM.

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

You can also pass a key directly with `--api-key`, or select a key file with `--api-key-file` / `KAGI_API_KEY_FILE`. Lookup order is `--api-key` / `KAGI_API_KEY`, then the selected key file, then the default user configuration file. An unreadable or empty selected file is an error, not a fallback.

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
kagi search 'query' --filters.region de --filters.after 2026-01-01
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

## HTTP server and LiteLLM

Run the server using the same Kagi key configuration as the CLI:

```sh
kagi serve
kagi serve --listen-address 127.0.0.1:3001
```

The default listener is `127.0.0.1:3000`. `KAGI_LISTEN_ADDRESS`, `KAGI_BASE_URL`, and `RUST_LOG` configure the listener, upstream API URL, and logging respectively. Corresponding flags are `--listen-address`, `--base-url`, and `--log-filter`.

`POST /search` accepts Perplexity search requests. `GET /health` is a local liveness check that makes no upstream call. Inbound bearer headers are ignored: the server always uses its configured Kagi key. There is no inbound authentication, so keep the listener private or put it behind an authenticated reverse proxy. Anyone who can reach it can spend your Kagi credits.

```sh
curl --fail http://127.0.0.1:3000/search \
  -H 'Content-Type: application/json' \
  -d '{"query":"rust tokio graceful shutdown","max_results":3}'
```

Configure LiteLLM's existing Perplexity search provider with our base URL and a dummy key. LiteLLM requires a nonempty key even though this server does not:

```python
import litellm

response = litellm.search(
    search_provider="perplexity",
    api_base="http://127.0.0.1:3000",
    api_key="unused",
    query="rust tokio graceful shutdown",
    max_results=3,
)
print(response.results)
```

For the LiteLLM proxy, add a named search tool to its configuration:

```yaml
search_tools:
  - search_tool_name: kagi
    litellm_params:
      search_provider: perplexity
      api_base: http://127.0.0.1:3000
      api_key: unused
```

Call the proxy's `POST /search` with `{"search_tool_name":"kagi","query":"rust","max_results":3}`. The base URL must be reachable from the LiteLLM process; a container's loopback is not the host's loopback. This is the search API, not Perplexity's Sonar chat API.

Supported parameters:

| Parameter | Behavior |
| --- | --- |
| `query` | A nonempty string or an array of 1-5 nonempty strings. Each query makes a separately billed Kagi search. |
| `max_results` | 1-20, default 10. Multi-query results are interleaved, deduplicated by URL, and capped at this total. |
| `search_domain_filter` | Up to 20 hostnames, mapped to Kagi lens inclusions or exclusions (`-example.com`) and `site:` operators. Returned URLs are also checked locally. URLs and paths are not supported. |
| `country` | Two-letter country code, normalized to lowercase for Kagi's `filters.region`. Kagi validates supported regions. |
| `max_tokens_per_page` | Accepted (1-1,000,000) but ignored. Responses use Kagi snippets without additional paid page extraction. |

Optional parameters can be omitted or `null`. Unsupported fields and invalid values produce JSON errors rather than silently dropping search constraints. Separate publication/update filters are not supported because Kagi does not distinguish them. The response's `date` contains Kagi's creation-or-update timestamp when available; `last_updated` is `null`.

Searches have a 60-second deadline, a 64 KiB request body limit, and a limit of 16 concurrent batches. Excess concurrent requests receive `503`, Kagi rate limits receive `429`, upstream failures receive `502`, and timeouts receive `504`. `SIGINT` and `SIGTERM` drain active searches before exit. Queries, credentials, and upstream response bodies are not logged.

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

For a system-wide server, import `nixosModules.default`:

```nix
{
  imports = [ inputs.kagi.nixosModules.default ];

  services.kagi = {
    enable = true;
    apiKeyFile = "/run/secrets/kagi-api-key";
    # listenAddress = "127.0.0.1:3000";
    # logFilter = "info";
  };
}
```

The module runs `kagi.service` as a hardened dynamic user, with the key supplied through systemd `LoadCredential`. It does not read your personal home directory. Point `apiKeyFile` at an existing key or an agenix/sops-nix runtime secret; the file contents stay out of the Nix store. `baseUrl`, `package`, and `shutdownTimeout` are also configurable. `openFirewall` defaults to `false`; enabling it exposes an unauthenticated, billable API on the configured listener.

`nix flake check` includes a NixOS VM test of the service, credential loading, HTTP translation, and clean shutdown against a mock upstream.

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

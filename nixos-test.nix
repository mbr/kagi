{ appModule }:
{
  name = "kagi-search-service";
  nodes.machine =
    { lib, pkgs, ... }:
    let
      upstream = pkgs.writeText "kagi-test-upstream.py" ''
        """Provides a deterministic Kagi upstream without network access or credits."""
        import json
        from http.server import BaseHTTPRequestHandler, HTTPServer

        class Handler(BaseHTTPRequestHandler):
            """Validates upstream authentication and search translation."""

            def do_POST(self):
                """Returns a web result after checking the adapter's request."""
                body = json.loads(self.rfile.read(int(self.headers["Content-Length"])))
                assert self.path == "/search"
                assert self.headers["Authorization"] == "Bearer test-kagi-key"
                assert body == {
                    "query": "rust", "format": "json", "workflow": "search", "limit": 1
                }, body
                response = json.dumps({"data": {"search": [{
                    "title": "Rust", "url": "https://rust-lang.org", "snippet": "Rust language"
                }]}}).encode()
                self.send_response(200)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(response)))
                self.end_headers()
                self.wfile.write(response)

        HTTPServer(("127.0.0.1", 9000), Handler).serve_forever()
      '';
    in
    {
      imports = [ appModule ];
      services.kagi = {
        enable = true;
        apiKeyFile = "/run/kagi-test-key";
        baseUrl = "http://127.0.0.1:9000";
        listenAddress = "[::1]:3000";
      };
      systemd.services.kagi.wantedBy = lib.mkForce [ ];
      systemd.services.kagi-test-upstream = {
        wantedBy = [ "multi-user.target" ];
        serviceConfig.ExecStart = "${pkgs.python3}/bin/python ${upstream}";
      };
      environment.systemPackages = [ pkgs.curl ];
      system.stateVersion = "26.05";
    };
  testScript = ''
    import json

    start_all()
    machine.wait_for_unit("kagi-test-upstream.service")
    machine.succeed("install -m 0600 /dev/null /run/kagi-test-key; printf test-kagi-key > /run/kagi-test-key")
    machine.succeed("systemctl start kagi.service")
    machine.wait_for_unit("kagi.service")
    machine.wait_until_succeeds("curl --noproxy '*' --fail --silent 'http://[::1]:3000/health'")

    for header in ["", "-H 'Authorization: Bearer dummy'"]:
        response = json.loads(machine.succeed(
            "curl --noproxy '*' --fail --silent 'http://[::1]:3000/search' "
            "-H 'Content-Type: application/json' " + header +
            " -d '{\"query\":\"rust\",\"max_results\":1,\"max_tokens_per_page\":1024}'"
        ))
        assert response["id"]
        assert response["results"][0]["title"] == "Rust", response
        assert response["results"][0]["snippet"] == "Rust language", response

    assert machine.succeed("systemctl show kagi.service -p DynamicUser --value").strip() == "yes"
    assert machine.succeed("systemctl show kagi.service -p ProtectHome --value").strip() == "yes"
    machine.succeed("systemctl stop kagi.service")
    assert machine.succeed("systemctl show kagi.service -p ExecMainStatus --value").strip() == "0"
    machine.succeed("systemctl start kagi.service")
    machine.wait_until_succeeds("curl --noproxy '*' --fail --silent 'http://[::1]:3000/health'")
    machine.fail("journalctl -u kagi.service | grep -F test-kagi-key")
  '';
}

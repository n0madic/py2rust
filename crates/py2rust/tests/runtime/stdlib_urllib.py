# Runtime coverage for lightweight urllib support.

import os
import urllib
from urllib.parse import urlparse, quote, unquote, urljoin, urlencode, parse_qs
from urllib.request import urlopen

# -----------------------------------------------------------------------------
# Module namespace attributes (`urllib.parse`, `urllib.request`)
# -----------------------------------------------------------------------------

assert urllib.parse.quote("a b") == "a%20b"

# -----------------------------------------------------------------------------
# urllib.parse.urlparse + ParseResult fields/geturl
# -----------------------------------------------------------------------------

parsed1 = urlparse("https://example.com/path/to?q=1#frag")
assert parsed1.scheme == "https"
assert parsed1.netloc == "example.com"
assert parsed1.path == "/path/to"
assert parsed1.query == "q=1"
assert parsed1.fragment == "frag"
assert parsed1.geturl() == "https://example.com/path/to?q=1#frag"

parsed2 = urlparse("/only/path")
assert parsed2.scheme == ""
assert parsed2.netloc == ""
assert parsed2.path == "/only/path"
assert parsed2.query == ""
assert parsed2.fragment == ""

parsed3 = urlparse("?a=1&b=2")
assert parsed3.path == ""
assert parsed3.query == "a=1&b=2"

parsed4 = urlparse("http://localhost:8080/api/v1")
assert parsed4.scheme == "http"
assert parsed4.netloc == "localhost:8080"
assert parsed4.path == "/api/v1"

parsed5 = urlparse("ftp://files.example.com/pub/file.txt")
assert parsed5.scheme == "ftp"
assert parsed5.netloc == "files.example.com"
assert parsed5.path == "/pub/file.txt"

parsed6 = urlparse("https://user:pass@example.com/secure")
assert parsed6.netloc == "user:pass@example.com"

parsed7 = urlparse("/just/a/path")
assert parsed7.scheme == ""
assert parsed7.path == "/just/a/path"

parsed8 = urlparse("?key=value&foo=bar")
assert parsed8.query == "key=value&foo=bar"

parsed9 = urlparse("")
assert parsed9.scheme == ""
assert parsed9.netloc == ""
assert parsed9.path == ""

print("urlparse tests passed")

# -----------------------------------------------------------------------------
# quote/unquote
# -----------------------------------------------------------------------------

assert quote("hello world") == "hello%20world"
assert quote("a/b c") == "a/b%20c"
assert quote("a/b c", "") == "a%2Fb%20c"
assert quote("a=b&c=d") == "a%3Db%26c%3Dd"
assert quote("ABC123") == "ABC123"
assert quote("hello%world") == "hello%25world"
assert quote("") == ""

assert unquote("hello%20world") == "hello world"
assert unquote("a%3Db%26c%3Dd") == "a=b&c=d"
assert unquote("hello+world") == "hello world"
assert unquote("hello%2fworld") == "hello/world"
assert unquote("already decoded") == "already decoded"
assert unquote("") == ""

print("quote/unquote tests passed")

# -----------------------------------------------------------------------------
# urljoin
# -----------------------------------------------------------------------------

assert urljoin("https://example.com/a/", "b") == "https://example.com/a/b"
assert urljoin("https://example.com/a/b", "/c") == "https://example.com/c"
assert urljoin("https://example.com/a/b/", "../c") == "https://example.com/a/c"
assert urljoin("https://example.com/a/b/c/", "../../d") == "https://example.com/a/d"
assert urljoin("https://example.com/path", "?query=1") == "https://example.com/path?query=1"
assert urljoin("https://example.com/", "https://other.com/x") == "https://other.com/x"
assert urljoin("https://example.com/path", "") == "https://example.com/path"

print("urljoin tests passed")

# -----------------------------------------------------------------------------
# urlencode / parse_qs
# -----------------------------------------------------------------------------

params1: dict[str, str] = {"key": "value"}
assert urlencode(params1) == "key=value"

params2: dict[str, str] = {"a": "1", "msg": "hello world"}
encoded2 = urlencode(params2)
assert "a=1" in encoded2
assert "msg=hello%20world" in encoded2
assert "&" in encoded2

params3: dict[str, str] = {}
assert urlencode(params3) == ""

parsed_qs1 = parse_qs("key=value")
assert parsed_qs1["key"][0] == "value"

parsed_qs2 = parse_qs("a=1&a=2")
assert len(parsed_qs2["a"]) == 2
assert parsed_qs2["a"][0] == "1"
assert parsed_qs2["a"][1] == "2"
first_a = parsed_qs2["a"][0]
second_a = parsed_qs2["a"][1]
assert first_a + second_a == "12"

parsed_qs3 = parse_qs("msg=hello+world")
assert parsed_qs3["msg"][0] == "hello world"

parsed_qs4 = parse_qs("foo=bar&baz=qux")
assert parsed_qs4["foo"][0] == "bar"
assert parsed_qs4["baz"][0] == "qux"

parsed_qs5 = parse_qs("?x=1")
assert parsed_qs5["x"][0] == "1"

parsed_qs6 = parse_qs("")
assert len(parsed_qs6) == 0

# Optional dict indexing regression coverage (same value indexed repeatedly).
maybe_qs: dict[str, list[str]] | None = parse_qs("k=v")
assert maybe_qs["k"][0] == "v"
same_k = maybe_qs["k"][0]
assert same_k + same_k == "vv"

# Integration check: parse, query parse, reassemble.
original_url = "https://api.example.com/v1/users?page=1&limit=10#results"
parsed_int = urlparse(original_url)
assert parsed_int.scheme == "https"
assert parsed_int.netloc == "api.example.com"
assert parsed_int.path == "/v1/users"
query_params = parse_qs(parsed_int.query)
assert query_params["page"][0] == "1"
assert query_params["limit"][0] == "10"
assert parsed_int.geturl() == original_url

print("urlencode/parse_qs tests passed")

# -----------------------------------------------------------------------------
# urllib.request.urlopen (covers file:// and data:text/plain, in offline runtime tests)
# -----------------------------------------------------------------------------

tmp_path = "/tmp/py2rust_urllib_runtime_fixture.txt"
with open(tmp_path, "w") as f:
    _ = f.write("line1\\nline2")

abs_path = os.path.abspath(tmp_path)
file_url = "file://" + abs_path

try:
    response1 = urlopen(file_url)
    assert response1.status == 200
    assert response1.getcode() == 200
    assert response1.geturl() == file_url
    body1 = response1.read()
    assert "line1" in body1
    assert "line2" in body1
    headers1 = response1.headers
    assert isinstance(headers1, dict)
    assert "content-length" in headers1

    response2 = urlopen(file_url, data=None, timeout=5)
    assert response2.status == 200

    response3 = urlopen("data:text/plain,hello%20world")
    assert response3.status == 200
    assert response3.read() == "hello world"

    response4 = urllib.request.urlopen("data:text/plain,module-call")
    assert response4.getcode() == 200
    assert response4.read() == "module-call"

    # Real network requests (httpbin). In offline environments we skip this block.
    network_ok = True
    try:
        response_get = urlopen("https://httpbin.org/get", None, 10.0)
        assert response_get.status == 200
        assert "httpbin.org" in response_get.url
        body_get = response_get.read()
        assert len(body_get) > 0
        assert response_get.getcode() == 200
        assert response_get.geturl() == response_get.url
        headers_get = response_get.headers
        assert isinstance(headers_get, dict)

        response_post = urlopen("https://httpbin.org/post", b"key=value", 10.0)
        assert response_post.status == 200

        response_404 = urlopen("https://httpbin.org/status/404", None, 10.0)
        assert response_404.status == 404
    except IOError:
        network_ok = False
    if not network_ok:
        print("urllib.request network tests skipped (offline)")

    caught_unsupported: bool = False
    try:
        _ = urlopen("ftp://example.com/resource")
    except ValueError:
        caught_unsupported = True
    assert caught_unsupported
finally:
    if os.path.exists(tmp_path):
        os.remove(tmp_path)

print("urllib.request tests passed")
print("All urllib tests passed!")

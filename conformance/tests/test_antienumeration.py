"""Anti-enumeration (ADR-0004/0012): prefix queries and unknown serials never
match (404); a suffixed variant of the portfolio serial matches (200)."""


def test_prefix_queries_are_404(client, target, serial):
    for q in [serial[:3], serial[:-1], serial + "X"]:
        r = client.get(f"{target}/v1/serials/{q}", timeout=10)
        assert r.status_code == 404, (
            f"anti-enumeration: query {q!r} must answer 404 (not found)"
        )


def test_suffixed_variant_is_200(client, target, serial):
    r = client.get(f"{target}/v1/serials/{serial}-X9", timeout=10)
    assert r.status_code == 200, "suffixed variant of the portfolio serial must match"
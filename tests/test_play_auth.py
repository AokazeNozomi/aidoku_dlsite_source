"""
Test DLsite Play auth flows to identify which steps are needed.

Usage:
    python tests/test_play_auth.py --xsrf-token TOKEN --play-session SESSION
    python tests/test_play_auth.py --test authorize-only ...
    python tests/test_play_auth.py --test direct ...
    python tests/test_play_auth.py --test bug ...
"""

import argparse
import json
import sys
import requests

PLAY_API = "https://play.dlsite.com/api/v3"
PLAY_LOGIN_URL = "https://play.dlsite.com/login/"
PLAY_AUTHORIZE_URL = "https://play.dlsite.com/api/authorize"
AIOHTTP_UA = "Python/3.12 aiohttp/3.11.16"
REFERER = "https://play.dlsite.com/"


def make_cookie_header(xsrf: str, session: str) -> str:
    return f"XSRF-TOKEN={xsrf}; play_session={session}"


def get_sales(cookie_header: str) -> tuple[int, str]:
    """GET /api/v3/content/sales — the actual data request."""
    resp = requests.get(
        f"{PLAY_API}/content/sales?last=0",
        headers={
            "User-Agent": AIOHTTP_UA,
            "Accept": "application/json",
            "Cookie": cookie_header,
        },
        allow_redirects=False,
    )
    return resp.status_code, resp.text[:200]


def get_login_page(cookie_header: str) -> tuple[int, dict[str, str]]:
    """GET /login/ — the bootstrap step that rotates the session."""
    resp = requests.get(
        PLAY_LOGIN_URL,
        headers={
            "User-Agent": AIOHTTP_UA,
            "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            "Cookie": cookie_header,
        },
        allow_redirects=False,
    )
    set_cookies = {}
    for cookie in resp.cookies:
        set_cookies[cookie.name] = cookie.value
    return resp.status_code, set_cookies


def get_authorize(cookie_header: str) -> tuple[int, dict[str, str]]:
    """GET /api/authorize — binds session to API."""
    resp = requests.get(
        PLAY_AUTHORIZE_URL,
        headers={
            "User-Agent": AIOHTTP_UA,
            "Referer": REFERER,
            "Accept": "*/*",
            "Cookie": cookie_header,
        },
        allow_redirects=False,
    )
    set_cookies = {}
    for cookie in resp.cookies:
        set_cookies[cookie.name] = cookie.value
    return resp.status_code, set_cookies


def test_direct(xsrf: str, session: str) -> bool:
    """Use WebView cookies directly — no bootstrap at all."""
    print("=" * 60)
    print("TEST: Direct API call (no bootstrap)")
    print("=" * 60)

    cookie_header = make_cookie_header(xsrf, session)
    print(f"\n1. Cookies:")
    print(f"   XSRF-TOKEN = {xsrf[:20]}...")
    print(f"   play_session = {session[:20]}...")

    print(f"\n2. GET /api/v3/content/sales...")
    status, body = get_sales(cookie_header)
    print(f"   HTTP {status}")
    if status == 200:
        try:
            entries = json.loads(body)
            print(f"   {len(entries)} sales entries")
        except json.JSONDecodeError:
            print(f"   {body[:100]}...")
        print(f"\n   PASS: direct API call works")
        return True
    else:
        print(f"   {body}")
        print(f"\n   FAIL: direct API call returned {status}")
        return False


def test_authorize_only(xsrf: str, session: str) -> bool:
    """Call /api/authorize (skip /login/), persist Set-Cookie, then API call."""
    print("=" * 60)
    print("TEST: Authorize only (skip /login/)")
    print("=" * 60)

    cookie_header = make_cookie_header(xsrf, session)
    print(f"\n1. Cookies:")
    print(f"   XSRF-TOKEN = {xsrf[:20]}...")
    print(f"   play_session = {session[:20]}...")

    print(f"\n2. GET /api/authorize...")
    status, new_cookies = get_authorize(cookie_header)
    print(f"   HTTP {status}")
    if new_cookies:
        print(f"   Set-Cookie: {list(new_cookies.keys())}")
    else:
        print(f"   Set-Cookie: (none)")

    current_xsrf = new_cookies.get("XSRF-TOKEN", xsrf)
    current_session = new_cookies.get("play_session", session)
    if current_xsrf != xsrf:
        print(f"   XSRF-TOKEN rotated")
    if current_session != session:
        print(f"   play_session rotated")

    cookie_header = make_cookie_header(current_xsrf, current_session)

    print(f"\n3. GET /api/v3/content/sales...")
    status, body = get_sales(cookie_header)
    print(f"   HTTP {status}")
    if status == 200:
        try:
            entries = json.loads(body)
            print(f"   {len(entries)} sales entries")
        except json.JSONDecodeError:
            print(f"   {body[:100]}...")
        print(f"\n   PASS: authorize-only flow works")
        return True
    else:
        print(f"   {body}")
        print(f"\n   FAIL: API call returned {status}")
        return False


def test_bug(xsrf: str, session: str) -> bool:
    """Full bootstrap: /login/ + /api/authorize + API call."""
    print("=" * 60)
    print("TEST: Full bootstrap (old code - /login/ + /authorize)")
    print("=" * 60)

    cookie_header = make_cookie_header(xsrf, session)
    print(f"\n1. Cookies:")
    print(f"   XSRF-TOKEN = {xsrf[:20]}...")
    print(f"   play_session = {session[:20]}...")

    print(f"\n2. GET /login/...")
    status, new_cookies = get_login_page(cookie_header)
    print(f"   HTTP {status}")
    if new_cookies:
        print(f"   Set-Cookie: {list(new_cookies.keys())}")
    else:
        print(f"   Set-Cookie: (none)")

    current_xsrf = new_cookies.get("XSRF-TOKEN", xsrf)
    current_session = new_cookies.get("play_session", session)
    if current_xsrf != xsrf:
        print(f"   XSRF-TOKEN rotated")
    if current_session != session:
        print(f"   play_session rotated")

    cookie_header = make_cookie_header(current_xsrf, current_session)

    print(f"\n3. GET /api/authorize...")
    status, new_cookies = get_authorize(cookie_header)
    print(f"   HTTP {status}")
    if new_cookies:
        print(f"   Set-Cookie: {list(new_cookies.keys())}")
    else:
        print(f"   Set-Cookie: (none)")

    current_xsrf = new_cookies.get("XSRF-TOKEN", current_xsrf)
    current_session = new_cookies.get("play_session", current_session)
    if "play_session" in new_cookies:
        print(f"   play_session rotated again")

    cookie_header = make_cookie_header(current_xsrf, current_session)

    print(f"\n4. GET /api/v3/content/sales...")
    status, body = get_sales(cookie_header)
    print(f"   HTTP {status}")
    if status == 401:
        print(f"   {body}")
        print(f"\n   BUG REPRODUCED: full bootstrap kills session")
        return True
    else:
        print(f"   {body[:100]}...")
        print(f"\n   Bug NOT reproduced")
        return False


def main():
    parser = argparse.ArgumentParser(
        description="Test DLsite Play auth flows"
    )
    parser.add_argument("--xsrf-token", required=True, help="XSRF-TOKEN cookie value")
    parser.add_argument(
        "--play-session", required=True, help="play_session cookie value"
    )
    parser.add_argument(
        "--test",
        choices=["direct", "authorize-only", "bug", "all"],
        default="all",
        help="Which test to run (default: all)",
    )
    args = parser.parse_args()

    xsrf = args.xsrf_token
    session = args.play_session

    if args.test == "direct":
        ok = test_direct(xsrf, session)
        sys.exit(0 if ok else 1)

    if args.test == "authorize-only":
        ok = test_authorize_only(xsrf, session)
        sys.exit(0 if ok else 1)

    if args.test == "bug":
        test_bug(xsrf, session)
        sys.exit(0)

    # all: run non-destructive tests first, then destructive
    print("Running tests in order: direct → authorize-only → bug")
    print("(Each test uses FRESH cookies from args, but server-side")
    print("rotation from one test may affect the next.)\n")

    r1 = test_direct(xsrf, session)
    print()
    r2 = test_authorize_only(xsrf, session)
    print()
    r3 = test_bug(xsrf, session)

    print("\n" + "=" * 60)
    print("RESULTS")
    print("=" * 60)
    print(f"  Direct API call:       {'PASS' if r1 else 'FAIL'}")
    print(f"  Authorize-only + API:  {'PASS' if r2 else 'FAIL'}")
    print(f"  Full bootstrap + API:  {'BUG (401)' if r3 else 'OK'}")

    if not r1 and r2:
        print("\nConclusion: WebView cookies need /api/authorize before API use.")
        print("Fix: call /api/authorize after web login, persist Set-Cookie.")
    elif r1:
        print("\nConclusion: WebView cookies work directly (session already bound).")
    elif not r1 and not r2:
        print("\nConclusion: Cookies may be expired or invalid.")

    sys.exit(0 if (r1 or r2) else 1)


if __name__ == "__main__":
    main()

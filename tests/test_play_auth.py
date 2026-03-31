"""
Reproduce the play_session invalidation bug and verify the fix.

Usage:
    python tests/test_play_auth.py --xsrf-token TOKEN --play-session SESSION

Get these values from your browser:
    1. Log in to https://play.dlsite.com
    2. Open DevTools → Application → Cookies → play.dlsite.com
    3. Copy the XSRF-TOKEN and play_session values
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


def test_bug(xsrf: str, session: str) -> bool:
    """Reproduce the bug: prime_play_api_session invalidates the WebView session."""
    print("=" * 60)
    print("TEST: Reproducing the bug (old code path)")
    print("=" * 60)

    cookie_header = make_cookie_header(xsrf, session)
    print(f"\n1. Original cookies from WebView:")
    print(f"   XSRF-TOKEN = {xsrf[:20]}...")
    print(f"   play_session = {session[:20]}...")

    # Step 1: GET /login/ (like prime_play_api_session did)
    print(f"\n2. GET /login/ (bootstrap step 1)...")
    status, new_cookies = get_login_page(cookie_header)
    print(f"   HTTP {status}")
    if new_cookies:
        print(f"   Set-Cookie: {list(new_cookies.keys())}")
    else:
        print(f"   Set-Cookie: (none)")

    # Apply Set-Cookie like old code did
    current_xsrf = new_cookies.get("XSRF-TOKEN", xsrf)
    current_session = new_cookies.get("play_session", session)
    if current_xsrf != xsrf:
        print(f"   XSRF-TOKEN rotated: {xsrf[:20]}... → {current_xsrf[:20]}...")
    if current_session != session:
        print(f"   play_session rotated: {session[:20]}... → {current_session[:20]}...")

    cookie_header = make_cookie_header(current_xsrf, current_session)

    # Step 2: GET /api/authorize (like prime_play_api_session did)
    print(f"\n3. GET /api/authorize (bootstrap step 2)...")
    status, new_cookies = get_authorize(cookie_header)
    print(f"   HTTP {status}")
    if new_cookies:
        print(f"   Set-Cookie: {list(new_cookies.keys())}")
    else:
        print(f"   Set-Cookie: (none)")

    current_xsrf = new_cookies.get("XSRF-TOKEN", current_xsrf)
    current_session = new_cookies.get("play_session", current_session)
    if "play_session" in new_cookies:
        print(f"   play_session rotated again: → {current_session[:20]}...")

    cookie_header = make_cookie_header(current_xsrf, current_session)

    # Step 3: GET /api/v3/content/sales (the actual request)
    print(f"\n4. GET /api/v3/content/sales (data request)...")
    status, body = get_sales(cookie_header)
    print(f"   HTTP {status}")
    if status == 401:
        print(f"   Response: {body}")
        print(f"\n   BUG REPRODUCED: bootstrap invalidated the session")
        return True
    else:
        print(f"   Response: {body[:100]}...")
        print(f"\n   Bug NOT reproduced (session survived bootstrap)")
        return False


def test_fix(xsrf: str, session: str) -> bool:
    """Verify the fix: use WebView cookies directly without priming."""
    print("\n" + "=" * 60)
    print("TEST: Verifying the fix (new code path)")
    print("=" * 60)

    cookie_header = make_cookie_header(xsrf, session)
    print(f"\n1. Original cookies from WebView:")
    print(f"   XSRF-TOKEN = {xsrf[:20]}...")
    print(f"   play_session = {session[:20]}...")

    # Skip bootstrap entirely, go straight to API call
    print(f"\n2. GET /api/v3/content/sales (direct, no bootstrap)...")
    status, body = get_sales(cookie_header)
    print(f"   HTTP {status}")
    if status == 200:
        try:
            entries = json.loads(body)
            print(f"   Response: {len(entries)} sales entries")
        except json.JSONDecodeError:
            print(f"   Response: {body[:100]}...")
        print(f"\n   FIX VERIFIED: direct API call works without bootstrap")
        return True
    else:
        print(f"   Response: {body}")
        print(f"\n   Fix FAILED: direct API call also returns {status}")
        print(f"   (Cookies may be expired — try fresh ones from the browser)")
        return False


def main():
    parser = argparse.ArgumentParser(
        description="Test DLsite Play auth flow: reproduce bug and verify fix"
    )
    parser.add_argument("--xsrf-token", required=True, help="XSRF-TOKEN cookie value")
    parser.add_argument(
        "--play-session", required=True, help="play_session cookie value"
    )
    parser.add_argument(
        "--fix-only",
        action="store_true",
        help="Only test the fix (skip bug reproduction to preserve session)",
    )
    args = parser.parse_args()

    if args.fix_only:
        ok = test_fix(args.xsrf_token, args.play_session)
        sys.exit(0 if ok else 1)

    # Test the fix FIRST (non-destructive) then reproduce the bug (destructive)
    print("Running fix test first (non-destructive)...")
    print("Then reproducing the bug (will invalidate these cookies).\n")

    fix_ok = test_fix(args.xsrf_token, args.play_session)

    if not fix_ok:
        print("\nSkipping bug reproduction — cookies appear invalid.")
        sys.exit(1)

    print("\n" + "-" * 60)
    print("NOTE: The next test will INVALIDATE your cookies.")
    print("You will need to log in again after this test.")
    print("-" * 60)

    bug_reproduced = test_bug(args.xsrf_token, args.play_session)

    print("\n" + "=" * 60)
    print("RESULTS")
    print("=" * 60)
    print(f"  Fix works (direct API call):     {'PASS' if fix_ok else 'FAIL'}")
    print(f"  Bug reproduced (bootstrap kills): {'YES' if bug_reproduced else 'NO'}")

    if fix_ok and bug_reproduced:
        print("\nConclusion: Root cause confirmed. The bootstrap (GET /login/)")
        print("invalidates the WebView session. Removing it fixes the 401.")
    elif fix_ok and not bug_reproduced:
        print("\nConclusion: Fix works but bug didn't reproduce.")
        print("Session may have survived rotation — the fix is still correct")
        print("as it avoids unnecessary requests and rotation risk.")

    sys.exit(0 if fix_ok else 1)


if __name__ == "__main__":
    main()

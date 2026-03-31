import requests
import json

PLAY_API = "https://play.dlsite.com/api/v3"
AUTHORIZE_URL = "https://play.dlsite.com/api/authorize"
UA = "Python/3.12 aiohttp/3.11.16"
REFERER = "https://play.dlsite.com/"

with open("tests/full_cookies.txt") as f:
    cookie_header = f.read().strip()

ps = None
xsrf = None
for part in cookie_header.split("; "):
    if part.startswith("play_session="):
        ps = part.split("=", 1)[1]
        enc = "encrypted" if ps.startswith("eyJ") else "plain"
        print(f"play_session format: {enc} ({len(ps)} chars)")
    if part.startswith("XSRF-TOKEN="):
        xsrf = part.split("=", 1)[1]

print()
print("--- Test 1: Direct API call (all cookies, no authorize) ---")
r = requests.get(
    f"{PLAY_API}/content/sales?last=0",
    headers={"User-Agent": UA, "Accept": "application/json", "Cookie": cookie_header},
    allow_redirects=False,
)
print(f"HTTP {r.status_code}")
if r.status_code == 200:
    print(f"PASS: {len(json.loads(r.text))} entries")
else:
    print(f"FAIL: {r.text[:100]}")

print()
print("--- Test 2: All cookies + authorize then API ---")
r = requests.get(
    AUTHORIZE_URL,
    headers={"User-Agent": UA, "Referer": REFERER, "Accept": "*/*", "Cookie": cookie_header},
    allow_redirects=False,
)
print(f"Authorize HTTP {r.status_code}")
new_ps = None
for c in r.cookies:
    if c.name == "play_session":
        new_ps = c.value
        print(f"Rotated play_session: {new_ps[:30]}... ({len(new_ps)} chars)")
if new_ps and ps:
    updated = cookie_header.replace(f"play_session={ps}", f"play_session={new_ps}")
    r2 = requests.get(
        f"{PLAY_API}/content/sales?last=0",
        headers={"User-Agent": UA, "Accept": "application/json", "Cookie": updated},
        allow_redirects=False,
    )
    print(f"Sales HTTP {r2.status_code}")
    if r2.status_code == 200:
        print(f"PASS: {len(json.loads(r2.text))} entries")
    else:
        print(f"FAIL: {r2.text[:100]}")

print()
print("--- Test 3: Only 2 cookies + authorize (Aidoku simulation) ---")
if xsrf and ps:
    two_cookies = f"XSRF-TOKEN={xsrf}; play_session={ps}"
    r = requests.get(
        AUTHORIZE_URL,
        headers={"User-Agent": UA, "Referer": REFERER, "Accept": "*/*", "Cookie": two_cookies},
        allow_redirects=False,
    )
    print(f"Authorize HTTP {r.status_code}")
    new_ps2 = None
    for c in r.cookies:
        if c.name == "play_session":
            new_ps2 = c.value
            print(f"Rotated play_session: {new_ps2[:30]}... ({len(new_ps2)} chars)")
    if new_ps2:
        two_updated = f"XSRF-TOKEN={xsrf}; play_session={new_ps2}"
        r2 = requests.get(
            f"{PLAY_API}/content/sales?last=0",
            headers={"User-Agent": UA, "Accept": "application/json", "Cookie": two_updated},
            allow_redirects=False,
        )
        print(f"Sales HTTP {r2.status_code}")
        if r2.status_code == 200:
            print(f"PASS: {len(json.loads(r2.text))} entries")
        else:
            print(f"FAIL: {r2.text[:100]}")

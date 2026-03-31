#!/usr/bin/env python3
"""Fake browser for e2e tests.

Simulates OAuth login/connect/logout by extracting callback params from the
URL and hitting the local callback server directly.
"""
import json
import sys
import os
from urllib.parse import urlparse, parse_qs, urlencode
from urllib.request import urlopen

def main():
    # Last argument is the URL
    url = sys.argv[-1]
    parsed = urlparse(url)
    params = parse_qs(parsed.query)

    # Login flow: URL has redirect_uri and state
    if 'redirect_uri' in params and 'state' in params:
        redirect_uri = params['redirect_uri'][0]
        state = params['state'][0]
        sep = '&' if '?' in redirect_uri else '?'
        callback_url = f"{redirect_uri}{sep}code=e2e-auth-code&state={state}"
        urlopen(callback_url)
        return

    # Logout flow: URL has returnTo
    if 'returnTo' in params:
        try:
            urlopen(params['returnTo'][0])
        except Exception:
            pass
        return

    # Connect flow: read connect state from shared file
    state_dir = os.environ.get('TV_PROXY_CONFIG_DIR', '')
    connect_state_file = os.path.join(state_dir, 'e2e-connect-state.json')

    if os.path.exists(connect_state_file):
        with open(connect_state_file) as f:
            data = json.load(f)
        redirect_uri = data['redirect_uri']
        state = data['state']
        sep = '&' if '?' in redirect_uri else '?'
        callback_url = f"{redirect_uri}{sep}connect_code=e2e-connect-code&state={state}"
        urlopen(callback_url)
        return

    print(f"Fake browser: could not determine flow from URL: {url}", file=sys.stderr)
    sys.exit(1)

if __name__ == '__main__':
    main()

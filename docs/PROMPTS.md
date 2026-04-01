First update the `login` command, so that it prompts for domain, client id, and client secret similar to the nodejs version. it should be possible to run `tv-proxy login` without any env vars set and have it guide the user through the necessary inputs. it should be possible to skip the prompts by setting the env vars or passing CLI flags, but the default should be interactive prompting. 

Next, update the `init` command to remove the domain/client id prompts and instead just ask if the user wants to run the `login` flow if no config is detected. If they say yes, it should delegate to the `login` command which will handle the interactive prompting. Init should handle below steps.

1. Run `brew tap auth0/auth0-cli && brew install auth0` to install the `auth0` CLI if its not installed, which is a prerequisite for `tv-proxy`.
2. Run `npx configure-auth0-token-vault --  --flavor=refresh_token_exchange` and capture the application's client id from the output.
3. Update the app with callback urls and logout urls using the `auth0` CLI:
```
   auth0 apps update <client_id> \
  --callbacks "http://127.0.0.1:18484/callback,http://127.0.0.1:18485/callback,http://127.0.0.1:18486/callback,http://127.0.0.1:18487/callback,http://127.0.0.1:18488/callback,http://127.0.0.1:18489/callback" \
  --logout-urls "http://127.0.0.1:18484,http://127.0.0.1:18485,http://127.0.0.1:18486,http://127.0.0.1:18487,http://127.0.0.1:18488,http://127.0.0.1:18489"
```
1. Retrieve your application's client secret (needed during `tv-proxy login`):

```bash
auth0 apps show <client_id> --reveal-secrets
```
5. Run `tv-proxy login` and provide the domain, client id and client secret as flags.
6. print instructions for next steps, which is to use `tv-proxy connect` to create connections to third party APIs and `tv-proxy fetch` to make authenticated API calls.
   
Finally, update all relevant documentation (README, SKILL.md) to reflect the new login and init flow.

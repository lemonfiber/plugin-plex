//! Who holds Plex, and claiming it. A fresh Plex Media Server belongs to no plex.tv
//! account and says so to anybody at `/identity`. It answers its own claim API only to
//! the machine it runs on, so it is claimed as its image's first run claims it: the code
//! plex.tv showed the operator is exchanged at plex.tv under the identifier Plex presents
//! there, and the token that answers is written into the preferences Plex reads when it
//! starts. The claim is in force once Plex starts again.

use async_trait::async_trait;
use lemonfiber_ports::http::{Method, Request};
use lemonfiber_ports::service::{Claim, Failure, Owner, Ownership};
use serde::Deserialize;

use super::{as_a_server, preferences, Plex, Wrapped, IDENTITY};

/// What a server asked for its preferences with the token it keeps answers it with.
const PREFS: &str = "/:/prefs";

/// Where plex.tv exchanges a claim code for the token of the account that showed it.
const EXCHANGE: &str = "/api/claim/exchange";

const TOKEN_OPENS: &str = "<authentication-token>";

const TOKEN_CLOSES: &str = "</authentication-token>";

const REFUSED: u16 = 401;

const NO_CODE: &str = "no claim code from plex.tv was entered for this Plex";

#[derive(Deserialize)]
struct Identity {
    claimed: bool,
}

impl Plex {
    /// Whether an account has claimed Plex, or nothing where it does not answer.
    async fn claimed(&self) -> Result<Option<bool>, Failure> {
        let Ok(response) = self
            .endpoint
            .send(&self.request(Method::Get, IDENTITY, None))
            .await
        else {
            return Ok(None);
        };
        if response.status >= 500 {
            return Ok(None);
        }
        let identity: Wrapped<Identity> = self
            .endpoint
            .decode(&response, "whether Plex is claimed could not be read")?;
        Ok(Some(identity.container.claimed))
    }

    /// `Ours` where Plex keeps a token and takes it, and `Theirs` otherwise.
    async fn held_by(&self) -> Result<Owner, Failure> {
        let Some(token) = self.token().await? else {
            return Ok(Owner::Theirs);
        };
        let response = self
            .endpoint
            .send(&self.request(Method::Get, PREFS, Some(&token)))
            .await?;
        if response.status == REFUSED {
            return Ok(Owner::Theirs);
        }
        self.endpoint.expect_success(&response)?;
        Ok(Owner::Ours)
    }

    /// The token plex.tv exchanges `code` for, asked as the server `client`.
    async fn exchanged(&self, code: &str, client: &str) -> Result<String, Failure> {
        let headers = as_a_server(client);
        let asked = lemonfiber_adapter_kit::endpoint::form_encoded(&[("token", code)]);
        let response = self
            .accounts
            .send(&Request {
                method: Method::Post,
                url: self.accounts.url(&format!("{EXCHANGE}?{asked}")),
                headers,
                body: None,
                pinned: None,
            })
            .await?;
        if !response.is_success() {
            return Err(self.accounts.refusal(&response));
        }
        response
            .body
            .split_once(TOKEN_OPENS)
            .and_then(|(_, rest)| rest.split_once(TOKEN_CLOSES))
            .map(|(token, _)| token.trim().to_owned())
            .filter(|token| !token.is_empty() && !token.contains(['<', '>', '&', '"']))
            .ok_or_else(|| {
                self.accounts
                    .refused("plex.tv took the claim code and answered no token")
            })
    }

    /// Exchange the operator's code and write the token into Plex's preferences.
    async fn claimed_with_code(&self) -> Result<Claim, Failure> {
        let code = self
            .code
            .key()
            .map_err(|why| self.endpoint.refused(&format!("{NO_CODE}: {why}")))?;
        let claimable = self
            .preferences()
            .await?
            .as_deref()
            .and_then(preferences::claimable)
            .ok_or_else(|| {
                self.endpoint.refused(
                    "Plex's preferences name no identifier for plex.tv, so nothing can be claimed yet",
                )
            })?;
        let token = self.exchanged(&code, &claimable.client).await?;
        self.write_preferences(claimable.holding(&token)).await?;
        Ok(Claim::ON_RESTART)
    }
}

#[async_trait]
impl Ownership for Plex {
    async fn owner(&self) -> Result<Owner, Failure> {
        match self.claimed().await? {
            None => Ok(Owner::Settling),
            Some(false) => Ok(Owner::Open),
            Some(true) => self.held_by().await,
        }
    }

    async fn claim(&self) -> Result<Claim, Failure> {
        match self.claimed().await? {
            None => Ok(Claim::Held(Owner::Settling)),
            Some(true) => self.held_by().await.map(Claim::Held),
            Some(false) => self.claimed_with_code().await,
        }
    }
}

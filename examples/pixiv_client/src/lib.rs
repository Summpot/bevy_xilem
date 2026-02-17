use std::{collections::HashMap, time::UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bevy_ecs::prelude::{Component, Resource};
use chrono::Local;
use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

pub const APP_API_BASE: &str = "https://app-api.pixiv.net";
pub const ACCOUNTS_BASE: &str = "https://accounts.pixiv.net";
/// Reverse-engineered from APK (`fn/b.java`): `/idp-urls` is under app-api base.
pub const IDP_BASE: &str = APP_API_BASE;
pub const APP_VERSION: &str = "6.171.0";
pub const APP_OS: &str = "android";

/// Reverse-engineered from APK (`lm/e.java`)
pub const HASH_SECRET: &str = "28c1fdd170a5204386cb1313c7077b34f83e4aaf4aa829ce78c231e05b0bae2c";
/// Reverse-engineered from APK (`s80/f1.java`, `au/a.java`)
pub const CLIENT_ID: &str = "MOBrBDS8blbauoSck0ZfDbtuzpyT";
/// Reverse-engineered from APK (`s80/f1.java`, `au/a.java`)
pub const CLIENT_SECRET: &str = "lsACyCD94FhDUtGTXi3QzcFE2uU1hqtDaKeqrdwj";

/// Pixiv image host access requirement from APK network stack.
pub const REQUIRED_REFERER: &str = "https://app-api.pixiv.net/";

#[must_use]
pub fn x_client_time_now() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

#[must_use]
pub fn x_client_hash(time_string: &str) -> String {
    let seed = format!("{time_string}{HASH_SECRET}");
    format!("{:x}", md5::compute(seed))
}

/// Build an RFC 7636-compatible PKCE code verifier.
#[must_use]
pub fn generate_pkce_code_verifier() -> String {
    let now_nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let seed = format!(
        "pixiv-client-{now_nanos}-{}-{}",
        std::process::id(),
        HASH_SECRET
    );
    let digest = Sha256::digest(seed.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// Convert a PKCE code verifier into S256 code challenge.
#[must_use]
pub fn pkce_s256_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

/// Build the Pixiv app login URL used for browser-based OAuth login.
///
/// Reverse-engineered from APK (`s80/f1.java`):
/// `web/v1/login?code_challenge=...&code_challenge_method=S256&client=pixiv-android`
pub fn build_browser_login_url(code_challenge: &str) -> Result<String> {
    let mut url = reqwest::Url::parse(&format!("{APP_API_BASE}/web/v1/login"))
        .context("invalid Pixiv login URL")?;

    url.query_pairs_mut()
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("client", "pixiv-android");

    Ok(url.into())
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AuthSession {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthTokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub scope: String,
    pub user: Option<User>,
}

impl From<AuthTokenResponse> for AuthSession {
    fn from(value: AuthTokenResponse) -> Self {
        Self {
            access_token: value.access_token,
            refresh_token: value.refresh_token,
            token_type: value.token_type,
            expires_in: value.expires_in,
            scope: value.scope,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdpUrlResponse {
    #[serde(rename = "auth-token")]
    pub auth_token_url: String,
    #[serde(rename = "auth-token-redirect-uri")]
    pub auth_token_redirect_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Component)]
pub struct Tag {
    pub name: String,
    #[serde(default)]
    pub translated_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProfileImageUrls {
    pub medium: String,
}

impl<'de> Deserialize<'de> for ProfileImageUrls {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawProfileImageUrls {
            #[serde(default)]
            medium: Option<String>,
            #[serde(default, rename = "px_50x50")]
            px_50x50: Option<String>,
            #[serde(default, rename = "px_170x170")]
            px_170x170: Option<String>,
        }

        let raw = RawProfileImageUrls::deserialize(deserializer)?;
        let medium = raw
            .medium
            .or(raw.px_50x50)
            .or(raw.px_170x170)
            .ok_or_else(|| {
                serde::de::Error::custom(
                    "missing profile image URL: expected `medium` or `px_50x50`/`px_170x170`",
                )
            })?;

        Ok(Self { medium })
    }
}

fn deserialize_u64_from_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum U64Like {
        Number(u64),
        String(String),
    }

    match U64Like::deserialize(deserializer)? {
        U64Like::Number(value) => Ok(value),
        U64Like::String(value) => value.parse::<u64>().map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Component)]
pub struct User {
    #[serde(deserialize_with = "deserialize_u64_from_string_or_number")]
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub account: Option<String>,
    pub profile_image_urls: ProfileImageUrls,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrls {
    pub medium: String,
    pub large: String,
    pub square_medium: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaPageUrl {
    #[serde(default)]
    pub original_image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Component)]
pub struct Illust {
    pub id: u64,
    pub title: String,
    pub image_urls: ImageUrls,
    pub user: User,
    #[serde(default)]
    pub tags: Vec<Tag>,
    #[serde(default)]
    pub total_view: u64,
    #[serde(default)]
    pub total_bookmarks: u64,
    #[serde(default)]
    pub total_comments: u64,
    #[serde(default)]
    pub is_bookmarked: bool,
    #[serde(default)]
    pub page_count: u32,
    #[serde(default)]
    pub meta_single_page: Option<MetaPageUrl>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PixivResponse {
    #[serde(default)]
    pub illusts: Vec<Illust>,
    #[serde(default)]
    pub next_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DecodedImageRgba {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

#[derive(Clone, Resource)]
pub struct PixivApiClient {
    http: Client,
}

impl Default for PixivApiClient {
    fn default() -> Self {
        let http = Client::builder()
            .user_agent(format!("PixivAndroidApp/{APP_VERSION}"))
            .build()
            .expect("reqwest client should build");
        Self { http }
    }
}

impl PixivApiClient {
    fn app_headers(&self, req: RequestBuilder, bearer: Option<&str>) -> RequestBuilder {
        let x_time = x_client_time_now();
        let x_hash = x_client_hash(&x_time);

        let req = req
            .header("Accept-Language", "en")
            .header("app-accept-language", "en")
            .header("App-OS", APP_OS)
            .header("App-OS-Version", "14")
            .header("App-Version", APP_VERSION)
            .header("X-Client-Time", x_time)
            .header("X-Client-Hash", x_hash)
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded;charset=UTF-8",
            );

        if let Some(token) = bearer {
            req.header("Authorization", format!("Bearer {token}"))
        } else {
            req
        }
    }

    fn decode_json<T: DeserializeOwned>(response: reqwest::blocking::Response) -> Result<T> {
        let status = response.status();
        let body = response
            .text()
            .unwrap_or_else(|err| format!("<unreadable body: {err}>"));
        Self::decode_json_from_body(status, &body)
    }

    fn decode_json_from_body<T: DeserializeOwned>(
        status: reqwest::StatusCode,
        body: &str,
    ) -> Result<T> {
        if !status.is_success() {
            return Err(anyhow!("request failed: status={status}, body={body}"));
        }

        serde_json::from_str::<T>(body)
            .with_context(|| format!("failed to decode json: status={status}, body={body}"))
    }

    pub fn discover_idp_urls(&self) -> Result<IdpUrlResponse> {
        let url = format!("{IDP_BASE}/idp-urls");
        let req = self.app_headers(self.http.get(url), None);
        let response = req.send().context("discover idp-urls failed")?;
        Self::decode_json(response)
    }

    pub fn exchange_authorization_code(
        &self,
        auth_token_url: &str,
        code_verifier: &str,
        code: &str,
        redirect_uri: &str,
    ) -> Result<AuthTokenResponse> {
        let mut form = HashMap::<&str, String>::new();
        form.insert("code_verifier", code_verifier.to_string());
        form.insert("code", code.to_string());
        form.insert("grant_type", "authorization_code".to_string());
        form.insert("redirect_uri", redirect_uri.to_string());
        form.insert("client_id", CLIENT_ID.to_string());
        form.insert("client_secret", CLIENT_SECRET.to_string());
        form.insert("include_policy", "true".to_string());

        let req = self
            .app_headers(self.http.post(auth_token_url), None)
            .form(&form);
        let response = req.send().context("exchange authorization_code failed")?;
        Self::decode_json(response)
    }

    pub fn refresh_access_token(
        &self,
        auth_token_url: &str,
        refresh_token: &str,
    ) -> Result<AuthTokenResponse> {
        let mut form = HashMap::<&str, String>::new();
        form.insert("client_id", CLIENT_ID.to_string());
        form.insert("client_secret", CLIENT_SECRET.to_string());
        form.insert("grant_type", "refresh_token".to_string());
        form.insert("refresh_token", refresh_token.to_string());
        form.insert("include_policy", "true".to_string());

        let req = self
            .app_headers(self.http.post(auth_token_url), None)
            .form(&form);
        let response = req.send().context("refresh token failed")?;
        Self::decode_json(response)
    }

    pub fn recommended_illusts(&self, access_token: &str) -> Result<PixivResponse> {
        let url = format!(
            "{APP_API_BASE}/v1/illust/recommended?filter=for_android&include_ranking_illusts=true&include_privacy_policy=false"
        );
        let req = self.app_headers(self.http.get(url), Some(access_token));
        let response = req.send().context("recommended illusts failed")?;
        Self::decode_json(response)
    }

    pub fn ranking_illusts(&self, access_token: &str, mode: &str) -> Result<PixivResponse> {
        let url = format!("{APP_API_BASE}/v1/illust/ranking?filter=for_android&mode={mode}");
        let req = self.app_headers(self.http.get(url), Some(access_token));
        let response = req.send().context("ranking illusts failed")?;
        Self::decode_json(response)
    }

    pub fn search_illusts(&self, access_token: &str, word: &str) -> Result<PixivResponse> {
        let url = format!(
            "{APP_API_BASE}/v1/search/illust?filter=for_android&include_translated_tag_results=true&merge_plain_keyword_results=true&word={word}&search_target=partial_match_for_tags"
        );
        let req = self.app_headers(self.http.get(url), Some(access_token));
        let response = req.send().context("search illusts failed")?;
        Self::decode_json(response)
    }

    pub fn bookmark_illust(&self, access_token: &str, illust_id: u64) -> Result<()> {
        let url = format!("{APP_API_BASE}/v2/illust/bookmark/add");
        let mut form = HashMap::<&str, String>::new();
        form.insert("illust_id", illust_id.to_string());
        form.insert("restrict", "public".to_string());

        let req = self
            .app_headers(self.http.post(url), Some(access_token))
            .form(&form);
        let response = req.send().context("bookmark add failed")?;
        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(anyhow!("bookmark failed: status={status}, body={body}"));
        }
        Ok(())
    }

    pub fn download_image_rgba8(&self, image_url: &str) -> Result<DecodedImageRgba> {
        let response = self
            .http
            .get(image_url)
            .header("Referer", REQUIRED_REFERER)
            .send()
            .with_context(|| format!("image request failed: {image_url}"))?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow!(
                "image request failed with status {status} for {image_url}"
            ));
        }

        let bytes = response
            .bytes()
            .with_context(|| format!("failed to read image bytes: {image_url}"))?;
        let decoded = image::load_from_memory(&bytes)
            .with_context(|| format!("failed to decode image: {image_url}"))?
            .into_rgba8();

        Ok(DecodedImageRgba {
            width: decoded.width(),
            height: decoded.height(),
            rgba8: decoded.into_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_engineered_constants_are_stable() {
        assert_eq!(CLIENT_ID, "MOBrBDS8blbauoSck0ZfDbtuzpyT");
        assert_eq!(CLIENT_SECRET, "lsACyCD94FhDUtGTXi3QzcFE2uU1hqtDaKeqrdwj");
        assert_eq!(
            HASH_SECRET,
            "28c1fdd170a5204386cb1313c7077b34f83e4aaf4aa829ce78c231e05b0bae2c"
        );
    }

    #[test]
    fn x_client_hash_is_lower_hex_md5() {
        let sample_time = "2026-02-17T12:34:56+09:00";
        let hash = x_client_hash(sample_time);
        assert_eq!(hash.len(), 32);
        assert!(
            hash.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        );
    }

    #[test]
    fn pkce_challenge_matches_rfc_example() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = pkce_s256_challenge(verifier);
        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn browser_login_url_contains_expected_query() {
        let url = build_browser_login_url("abc123").expect("url should build");

        assert!(url.starts_with("https://app-api.pixiv.net/web/v1/login?"));
        assert!(url.contains("code_challenge=abc123"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("client=pixiv-android"));
        assert!(!url.contains("redirect_uri="));
        assert!(!url.contains("response_type="));
    }

    #[test]
    fn decode_json_error_includes_response_body() {
        let err = PixivApiClient::decode_json_from_body::<AuthTokenResponse>(
            reqwest::StatusCode::OK,
            "not-json-response",
        )
        .expect_err("invalid json should fail");

        let message = err.to_string();
        assert!(message.contains("failed to decode json"));
        assert!(message.contains("not-json-response"));
    }

    #[test]
    fn auth_token_response_accepts_string_user_id_and_px_profile_keys() {
        let body = r#"{
            "access_token": "token",
            "expires_in": 3600,
            "token_type": "bearer",
            "scope": "",
            "refresh_token": "refresh",
            "user": {
                "profile_image_urls": {
                    "px_16x16": "https://example.com/16.png",
                    "px_50x50": "https://example.com/50.png",
                    "px_170x170": "https://example.com/170.png"
                },
                "id": "33239622",
                "name": "summpot",
                "account": "user_knrk3528"
            }
        }"#;

        let parsed = PixivApiClient::decode_json_from_body::<AuthTokenResponse>(
            reqwest::StatusCode::OK,
            body,
        )
        .expect("auth response should parse");

        let user = parsed.user.expect("user should exist");
        assert_eq!(user.id, 33_239_622);
        assert_eq!(user.profile_image_urls.medium, "https://example.com/50.png");
    }
}

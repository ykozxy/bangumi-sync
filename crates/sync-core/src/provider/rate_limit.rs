use crate::model::Provider;

use super::ProviderHttpHeader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRateLimit {
    pub provider: Provider,
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub retry_after_seconds: Option<u64>,
}

impl ProviderRateLimit {
    pub fn should_backoff(&self) -> bool {
        self.retry_after_seconds.is_some() || self.remaining == Some(0)
    }
}

pub fn parse_provider_rate_limit(
    provider: Provider,
    headers: &[ProviderHttpHeader],
) -> ProviderRateLimit {
    ProviderRateLimit {
        provider,
        limit: parse_u32_header(headers, "x-ratelimit-limit"),
        remaining: parse_u32_header(headers, "x-ratelimit-remaining"),
        retry_after_seconds: parse_u64_header(headers, "retry-after"),
    }
}

fn parse_u32_header(headers: &[ProviderHttpHeader], name: &str) -> Option<u32> {
    header_value(headers, name)?.parse::<u32>().ok()
}

fn parse_u64_header(headers: &[ProviderHttpHeader], name: &str) -> Option<u64> {
    header_value(headers, name)?.parse::<u64>().ok()
}

fn header_value<'a>(headers: &'a [ProviderHttpHeader], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case(name))
        .map(|header| header.value.as_str())
}

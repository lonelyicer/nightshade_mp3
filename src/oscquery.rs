use crate::{
    error::{AppError, AppResult},
    model::OscQueryConfig,
};
use mdns_sd::{ServiceDaemon, ServiceEvent};
use reqwest::Client;
use serde::Deserialize;
use std::{net::Ipv4Addr, time::Duration};

const OSCQUERY_SERVICE: &str = "_oscjson._tcp.local.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OscEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize)]
struct HostInfo {
    #[serde(rename = "NAME")]
    name: Option<String>,

    #[serde(rename = "OSC_IP")]
    osc_ip: Option<String>,

    #[serde(rename = "OSC_PORT")]
    osc_port: Option<u16>,
}

pub struct OscQueryClient {
    config: OscQueryConfig,
    http: Client,
}

impl OscQueryClient {
    pub fn new(config: OscQueryConfig) -> AppResult<Self> {
        let http = Client::builder()
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_secs(2))
            .build()?;

        Ok(Self { config, http })
    }

    pub async fn discover(&self, timeout: Duration) -> AppResult<Option<OscEndpoint>> {
        if !self.config.enabled {
            return Ok(None);
        }

        self.discover_mdns(timeout).await
    }

    async fn discover_mdns(&self, timeout: Duration) -> AppResult<Option<OscEndpoint>> {
        let daemon = ServiceDaemon::new().map_err(mdns_error)?;

        let receiver = daemon.browse(OSCQUERY_SERVICE).map_err(mdns_error)?;

        let deadline = tokio::time::Instant::now() + timeout;

        let result = loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());

            if remaining.is_zero() {
                break None;
            }

            let event = match tokio::time::timeout(remaining, receiver.recv_async()).await {
                Ok(Ok(event)) => event,
                _ => break None,
            };

            let ServiceEvent::ServiceResolved(service) = event else {
                continue;
            };

            let fullname = service.get_fullname().to_ascii_lowercase();

            let service_is_vrchat = fullname.contains("vrchat");

            let address = preferred_address(service.get_addresses_v4());

            let Some(address) = address else {
                continue;
            };

            if let Ok(Some(endpoint)) = self
                .query_candidate(&address.to_string(), service.get_port(), service_is_vrchat)
                .await
            {
                break Some(endpoint);
            }
        };

        let _ = daemon.stop_browse(OSCQUERY_SERVICE);

        let _ = daemon.shutdown();

        Ok(result)
    }

    async fn query_candidate(
        &self,
        host: &str,
        query_port: u16,
        accept_unnamed_vrchat: bool,
    ) -> AppResult<Option<OscEndpoint>> {
        let url = format!("http://{host}:{query_port}/?HOST_INFO");

        let response = self.http.get(url).send().await?;

        if !response.status().is_success() {
            return Ok(None);
        }

        let info = response.json::<HostInfo>().await?;

        let name_is_vrchat = info
            .name
            .as_deref()
            .is_some_and(|name| name.to_ascii_lowercase().contains("vrchat"));

        if !name_is_vrchat && !accept_unnamed_vrchat {
            return Ok(None);
        }

        let host = info
            .osc_ip
            .filter(|value| !value.trim().is_empty() && value != "0.0.0.0")
            .unwrap_or_else(|| host.to_owned());

        let port = info.osc_port.filter(|port| *port > 0).unwrap_or(query_port);

        Ok(Some(OscEndpoint { host, port }))
    }
}

fn preferred_address(addresses: std::collections::HashSet<Ipv4Addr>) -> Option<Ipv4Addr> {
    addresses
        .iter()
        .copied()
        .find(Ipv4Addr::is_loopback)
        .or_else(|| addresses.into_iter().next())
}

fn mdns_error(error: mdns_sd::Error) -> AppError {
    AppError::Message(error.to_string())
}

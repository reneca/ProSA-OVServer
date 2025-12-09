use std::{collections::HashMap, convert::Infallible};

use bytes::{Buf as _, Bytes};
use chrono::{DateTime, Duration, Utc};
use http::{Method, Request, Response, StatusCode};
use http_body_util::{BodyExt as _, Full, combinators::BoxBody};
use hyper::body::Incoming;
use opentelemetry::KeyValue;
use prosa::core::{
    adaptor::Adaptor,
    proc::{ProcConfig as _, ProcSettings as _},
};
use prosa_fetcher::{
    adaptor::FetcherAdaptor,
    proc::{FetchAction, FetcherError, FetcherProc},
};
use serde::Deserialize;
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::transmission::{
    self,
    api::{SessionStats, Status, TorrentField, TorrentSetParams},
};

#[derive(Default, Debug)]
pub enum TorrentFetchState {
    #[default]
    SessionStats,
    TorrentGet,
    TorrentMut(Vec<transmission::api::Method>),
    End,
}

impl TorrentFetchState {
    /// Getter of the method to send
    pub fn get_method(&self) -> Option<transmission::api::Method> {
        match self {
            TorrentFetchState::SessionStats => Some(transmission::api::Method::SessionStats),
            TorrentFetchState::TorrentGet => Some(transmission::api::Method::TorrentGet(
                vec![
                    TorrentField::Id,
                    TorrentField::Name,
                    TorrentField::Status,
                    TorrentField::TrackerList,
                    TorrentField::AddedDate,
                    TorrentField::UploadLimited,
                ],
                None,
            )),
            TorrentFetchState::TorrentMut(methods) => methods.first().cloned(),
            _ => None,
        }
    }
}

#[derive(Default, Debug, Deserialize)]
struct TorrentSettings {
    /// List of allowed trackers. Allow the torrent to seed if it's in this list
    #[serde(default)]
    tracker_allowlist: Vec<String>,
    /// Remove the torrent after an amount of days
    #[serde(default)]
    remove_after: Option<u32>,
}

impl TorrentSettings {
    /// Method to check if the tracker is allowed
    /// return `true` if it's the case
    fn tracker_allowed(&self, tracker: String) -> bool {
        for tracker_allow in &self.tracker_allowlist {
            if tracker.contains(tracker_allow) {
                return true;
            }
        }

        false
    }

    /// Method to know if the torrent need to be removed
    fn need_removal(&self, added_date: DateTime<Utc>) -> bool {
        if let Some(days) = self.remove_after.map(|d| {
            Utc::now()
                .checked_sub_signed(Duration::days(d.into()))
                .unwrap()
        }) {
            added_date < days
        } else {
            false
        }
    }
}

#[derive(Adaptor)]
pub struct TorrentAdaptor {
    transmission_uri: hyper::Uri,
    settings: Option<TorrentSettings>,
    state: TorrentFetchState,
    session_id: Option<String>,
    torrent_count: watch::Sender<Vec<Status>>,
    torrent_stats: watch::Sender<SessionStats>,
}

impl<M> FetcherAdaptor<M> for TorrentAdaptor
where
    M: 'static
        + std::marker::Send
        + std::marker::Sync
        + std::marker::Sized
        + std::clone::Clone
        + std::fmt::Debug
        + prosa_utils::msg::tvf::Tvf
        + std::default::Default,
{
    fn new(proc: &FetcherProc<M>) -> Result<Self, FetcherError<M>>
    where
        Self: std::marker::Sized,
    {
        let (meter_status, watch_status) = watch::channel(Vec::new());
        let _observable_torrent_count =
            proc.get_proc_param()
                .meter("transmission")
                .u64_observable_gauge("prosa_transmission_torrent_count")
                .with_description("Count of all torrents")
                .with_callback(move |observer| {
                    let frequencies = watch_status.borrow().iter().copied().fold(
                        HashMap::new(),
                        |mut map, val| {
                            map.entry(val).and_modify(|frq| *frq += 1).or_insert(1u64);
                            map
                        },
                    );

                    for status in Status::iterator() {
                        if let Some(torrent_count) = frequencies.get(&status) {
                            observer.observe(
                                *torrent_count,
                                &[KeyValue::new("status", status.to_string())],
                            );
                        } else {
                            observer.observe(0, &[KeyValue::new("status", status.to_string())]);
                        }
                    }
                })
                .build();

        let (meter_stats, watch_stats) = watch::channel(SessionStats::default());
        let _observable_session_stats = proc
            .get_proc_param()
            .meter("transmission")
            .u64_observable_counter("prosa_transmission_session_stats")
            .with_description("Stats of the session")
            .with_callback(move |observer| {
                let stats = watch_stats.borrow();
                observer.observe(stats.uploaded_bytes, &[KeyValue::new("flow", "send")]);
                observer.observe(stats.downloaded_bytes, &[KeyValue::new("flow", "recv")]);
            })
            .build();

        Ok(Self {
            transmission_uri: "/transmission/rpc".parse::<hyper::Uri>().map_err(|e| {
                FetcherError::Other(format!("can't parse hyper::uri `/transmission/rpc` : {e}"))
            })?,
            settings: proc.settings.get_adaptor_config::<TorrentSettings>().ok(),
            state: TorrentFetchState::default(),
            session_id: None,
            torrent_count: meter_status,
            torrent_stats: meter_stats,
        })
    }

    fn fetch(&mut self) -> Result<FetchAction<M>, FetcherError<M>> {
        // Call HTTP to retrieve torrents with first state
        self.state = TorrentFetchState::default();
        Ok(FetchAction::Http)
    }

    fn create_http_request(
        &self,
        mut request_builder: http::request::Builder,
    ) -> Result<Request<BoxBody<hyper::body::Bytes, Infallible>>, FetcherError<M>> {
        if let Some(session_id) = &self.session_id {
            if let Some(method) = self.state.get_method() {
                request_builder = request_builder
                    .method(Method::POST)
                    .uri(self.transmission_uri.clone())
                    .header(hyper::header::CONNECTION, "keep-alive")
                    .header(hyper::header::CONTENT_TYPE, "application/json")
                    .header(hyper::header::ACCEPT, "application/json")
                    .header("X-Transmission-Session-Id", session_id);

                let request = request_builder.body(BoxBody::new(Full::new(Bytes::from(
                    serde_json::to_vec(&method).map_err(|e| {
                        FetcherError::Other(format!("can't serialize transmission method: {e}"))
                    })?,
                ))))?;

                Ok(request)
            } else {
                Err(FetcherError::Other(
                    "Can't get torrent method for call".to_string(),
                ))
            }
        } else {
            request_builder = request_builder
                .method(Method::GET)
                .uri(self.transmission_uri.clone())
                .header(hyper::header::CONNECTION, "keep-alive")
                .header(hyper::header::ACCEPT, "application/json");
            let request = request_builder.body(BoxBody::default())?;
            Ok(request)
        }
    }

    async fn process_http_response(
        &mut self,
        response: Result<Response<Incoming>, FetcherError<M>>,
    ) -> Result<FetchAction<M>, FetcherError<M>> {
        match response {
            Ok(response) => {
                if self.session_id.is_none() {
                    match response.status() {
                        StatusCode::OK | StatusCode::CONFLICT => {
                            if let Some(session_id) =
                                response.headers().get("x-transmission-session-id")
                            {
                                self.session_id = session_id.to_str().map(|s| s.to_string()).ok();
                            }

                            if self.session_id.is_some() {
                                // Go for next call
                                Ok(FetchAction::Http)
                            } else {
                                Err(FetcherError::Other(
                                    "Can't retrieve `x-transmission-session-id` from remote"
                                        .to_string(),
                                ))
                            }
                        }
                        code => Err(FetcherError::Other(format!(
                            "Receive error from HTTP remote for login: {code}"
                        ))),
                    }
                } else {
                    match response.status() {
                        StatusCode::OK => {
                            let server = response
                                .headers()
                                .get(http::header::SERVER)
                                .and_then(|s| s.to_str().ok().map(|h| h.to_string()));
                            let body = response
                                .collect()
                                .await
                                .map_err(|e| FetcherError::Hyper(e, server.unwrap_or_default()))?
                                .aggregate();
                            let api_resp: transmission::api::Response =
                                serde_json::from_reader(body.reader())
                                    .map_err(|e| FetcherError::Io(e.into()))?;

                            match &mut self.state {
                                TorrentFetchState::SessionStats => {
                                    if let Some(session_stats) = api_resp.arguments.cumulative_stats
                                    {
                                        let _ = self.torrent_stats.send(session_stats);
                                    }

                                    self.state = TorrentFetchState::TorrentGet;
                                    Ok(FetchAction::Http)
                                }
                                TorrentFetchState::TorrentGet => {
                                    let mut torrent_no_peer_list = Vec::new();
                                    let mut torrent_rm_list = Vec::new();
                                    let mut torrents_status = Vec::new();
                                    for torrent in api_resp.arguments.torrents {
                                        if let Some(status) = torrent.status {
                                            torrents_status.push(status);
                                        }

                                        if let Some(torrent_settings) = &self.settings
                                            && let Some(torrent_id) = torrent.id
                                        {
                                            if !torrent.upload_limited
                                                && torrent.tracker_list.is_some_and(|t| {
                                                    !torrent_settings.tracker_allowed(t)
                                                })
                                            {
                                                torrent_no_peer_list.push(torrent_id);
                                            } else if torrent
                                                .added_date
                                                .is_some_and(|d| torrent_settings.need_removal(d))
                                            {
                                                torrent_rm_list.push(torrent_id);
                                            }
                                        }
                                    }

                                    let _ = self.torrent_count.send(torrents_status);

                                    if !torrent_no_peer_list.is_empty()
                                        || !torrent_rm_list.is_empty()
                                    {
                                        let mut torrent_mut_list = Vec::new();

                                        if !torrent_no_peer_list.is_empty() {
                                            torrent_mut_list.push(
                                                transmission::api::Method::TorrentSet(Box::new(
                                                    TorrentSetParams {
                                                        ids: Some(torrent_no_peer_list),
                                                        peer_limit: Some(0),
                                                        upload_limit: Some(0),
                                                        upload_limited: Some(true),
                                                        ..Default::default()
                                                    },
                                                )),
                                            );
                                        }

                                        if !torrent_rm_list.is_empty() {
                                            torrent_mut_list.push(
                                                transmission::api::Method::TorrentRemove(
                                                    torrent_rm_list,
                                                    false,
                                                ),
                                            );
                                        }

                                        self.state =
                                            TorrentFetchState::TorrentMut(torrent_mut_list);
                                        Ok(FetchAction::Http)
                                    } else {
                                        self.state = TorrentFetchState::End;
                                        Ok(FetchAction::None)
                                    }
                                }
                                TorrentFetchState::TorrentMut(torrent_list) => {
                                    if let Some(method) = torrent_list.pop() {
                                        info!(
                                            "Method `{method:?}`, return with {}",
                                            api_resp.result
                                        );
                                    }

                                    if torrent_list.is_empty() {
                                        Ok(FetchAction::None)
                                    } else {
                                        Ok(FetchAction::Http)
                                    }
                                }
                                TorrentFetchState::End => Ok(FetchAction::None),
                            }
                        }
                        StatusCode::CONFLICT => {
                            warn!("Transmission session ID expired");
                            self.session_id = None;
                            // Ask for a new session id (it may expired)
                            Ok(FetchAction::Http)
                        }
                        code => Err(FetcherError::Other(format!(
                            "Receive error from HTTP remote: {code}, for state: {:?}",
                            self.state
                        ))),
                    }
                }
            }
            Err(FetcherError::Hyper(he, addr)) => {
                if he.is_canceled() {
                    debug!(addr = addr, "HTTP error {:?}", he);
                    Ok(FetchAction::None)
                } else {
                    warn!(addr = addr, "HTTP error {:?}", he);
                    Err(FetcherError::Hyper(he, addr))
                }
            }
            Err(e) => Err(e),
        }
    }
}

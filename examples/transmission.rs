use http::HeaderValue;
use prosa_ovserver::transmission::{
    self,
    api::{self, TorrentField},
};
use reqwest::Client;

async fn get_torrent(
    client: &Client,
    transmission_id: &HeaderValue,
) -> Result<reqwest::Response, reqwest::Error> {
    let torrent_get = transmission::api::Method::TorrentGet(
        vec![
            TorrentField::Id,
            TorrentField::Name,
            TorrentField::Status,
            TorrentField::TrackerList,
            TorrentField::AddedDate,
        ],
        None,
    );
    let torrent_get_json = serde_json::to_string(&torrent_get).unwrap();
    println!("Try to send: {torrent_get_json}");

    client
        .post("http://localhost:9091/transmission/rpc")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("x-transmission-session-id", transmission_id)
        .body(torrent_get_json)
        .send()
        .await
}

async fn get_stat(
    client: &Client,
    transmission_id: &HeaderValue,
) -> Result<reqwest::Response, reqwest::Error> {
    let session_stat = transmission::api::Method::SessionStats;
    let session_stat_json = serde_json::to_string(&session_stat).unwrap();
    println!("Try to send: {session_stat_json}");

    client
        .post("http://localhost:9091/transmission/rpc")
        .header(http::header::CONTENT_TYPE, "application/json")
        .header("x-transmission-session-id", transmission_id)
        .body(session_stat_json)
        .send()
        .await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let res = client
        .get("http://localhost:9091/transmission/rpc")
        .header(http::header::CONTENT_TYPE, "application/json")
        .send()
        .await?;

    let transmission_id = res.headers().get("x-transmission-session-id");
    if let Some(transmission_id) = transmission_id {
        let responses = [
            get_torrent(&client, transmission_id).await?,
            get_stat(&client, transmission_id).await?,
        ];

        for res in responses {
            if res.status() == 200
                && res
                    .headers()
                    .get(http::header::CONTENT_TYPE)
                    .is_some_and(|h| h.to_str().is_ok_and(|h| h.starts_with("application/json")))
            {
                let json_response = res.text().await?;
                println!("json response: {json_response}");
                let torrent_res: api::Response = serde_json::from_str(json_response.as_str())?;
                println!("Torrent: {torrent_res:?}")
            } else {
                println!("res = {res:?}");
                println!("body: {}", res.text().await?);
            }
        }
    }

    Ok(())
}

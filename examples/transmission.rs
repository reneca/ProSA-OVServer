use prosa_ovserver::transmission::{
    self,
    api::{self, TorrentField},
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let res = client
        .get("http://localhost:9091/transmission/rpc")
        .header("Content-Type", "application/json")
        .send()
        .await?;
    println!("res = {res:?}");

    let transmission_id = res.headers().get("x-transmission-session-id");
    if let Some(transmission_id) = transmission_id {
        let torrent_get = transmission::api::Method::TorrentGet(
            vec![
                TorrentField::Id,
                TorrentField::Name,
                TorrentField::TrackerList,
                TorrentField::AddedDate,
            ],
            None,
        );
        let torrent_get_json = serde_json::to_string(&torrent_get).unwrap();
        println!("Try to send: {torrent_get_json}");

        let res = client
            .post("http://localhost:9091/transmission/rpc")
            .header("Content-Type", "application/json")
            .header("x-transmission-session-id", transmission_id)
            .body(torrent_get_json)
            .send()
            .await?;

        if res.status() == 200
            && res
                .headers()
                .get("Content-Type")
                .is_some_and(|h| h == "application/json")
        {
            let torrent_res: api::Response = serde_json::from_str(res.text().await?.as_str())?;
            println!("Torrent: {torrent_res:?}")
        } else {
            println!("res = {res:?}");
            println!("body: {}", res.text().await?);
        }
    }

    Ok(())
}

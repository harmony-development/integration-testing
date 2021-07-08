use std::time::{Duration, Instant};

use harmony_rust_sdk::{
    api::{
        chat::{GetEmotePacksRequest, GetGuildListRequest, Place},
        exports::hrpc::{tracing, url::Url},
    },
    client::{
        api::{
            auth::*,
            chat::{
                channel::*,
                guild::{CreateGuild, UpdateGuildInformation},
                message::*,
                permissions::{QueryPermissions, QueryPermissionsSelfBuilder},
                profile::*,
                *,
            },
            *,
        },
        error::*,
        *,
    },
};

const SERVER_ADDR: &str = "https://localhost:2289";
const EMAIL: &str = "test@test.org";
const PASSWORD: &str = "123456789Ab";

const SCHERZO_DATA: BenchData = BenchData {
    id: "scherzo",
    guild_id: 0,
    channel_id: 0,
};

const LEGATO_DATA: BenchData = BenchData {
    id: "legato",
    guild_id: 0,
    channel_id: 0,
};

#[derive(Copy, Clone)]
struct BenchData {
    id: &'static str,
    guild_id: u64,
    channel_id: u64,
}

#[tokio::main]
async fn main() -> ClientResult<()> {
    bench_send_msgs(SCHERZO_DATA).await?;
    bench_send_msgs(LEGATO_DATA).await?;
    Ok(())
}

async fn bench_send_msgs(data: BenchData) -> ClientResult<()> {
    let sent_10_msg = send_messages(10, data).await?;
    let sent_100_msg = send_messages(100, data).await?;
    let sent_1000_msg = send_messages(1000, data).await?;
    let sent_10000_msg = send_messages(10000, data).await?;
    println!(
        "{} send messages results:\n10 msgs: {}\n100 msgs: {}\n1000 msgs: {}\n10000 msgs: {}",
        data.id,
        sent_10_msg.as_secs_f64(),
        sent_100_msg.as_secs_f64(),
        sent_1000_msg.as_secs_f64(),
        sent_10000_msg.as_secs_f64()
    );
    Ok(())
}

async fn send_messages(num: usize, data: BenchData) -> ClientResult<Duration> {
    let client = create_new_client().await?;
    let since = Instant::now();
    for i in 0..num {
        send_message(
            &client,
            SendMessage::new(data.guild_id, data.channel_id).text(i),
        )
        .await?;
    }
    Ok(since.elapsed())
}

async fn create_new_client() -> ClientResult<Client> {
    let client = Client::new(SERVER_ADDR.parse().unwrap(), None).await?;
    client.begin_auth().await?;
    client.next_auth_step(AuthStepResponse::Initial).await?;
    client
        .next_auth_step(AuthStepResponse::login_choice())
        .await?;
    client
        .next_auth_step(AuthStepResponse::login_form(EMAIL, PASSWORD))
        .await?;
    Ok(client)
}

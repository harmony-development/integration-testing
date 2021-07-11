use std::array::IntoIter;

use harmony_rust_sdk::{
    api::{
        auth::{auth_step::Step, next_step_request::form_fields::Field},
        chat::{EventSource, GetGuildListRequest, InviteId, JoinGuildRequest},
        exports::hrpc::futures_util::future::try_join_all,
    },
    client::{
        api::{
            auth::*,
            chat::{
                channel::get_guild_channels,
                guild::{create_guild, get_guild_list, join_guild, CreateGuild},
                invite::{create_invite, CreateInvite},
                message::*,
                GuildId,
            },
        },
        error::*,
        *,
    },
};
use tokio::{
    task::JoinError,
    time::{Duration, Instant},
};

const SERVER_ADDR: &str = "https://localhost:2289";
const PASSWORD: &str = "123456789Ab";

#[derive(Copy, Clone, Default)]
struct BenchData {
    guild_id: u64,
    channel_id: u64,
}

#[tokio::main]
async fn main() -> ClientResult<()> {
    match std::env::args().nth(1).unwrap().as_str() {
        "send_messages" => {
            let datas = (1..=4)
                .map(|num| format!("test{}@test.org", num))
                .collect::<Vec<_>>();

            let (first, second, third, fourth) = tokio::try_join!(
                bench_send_msgs(datas[0].as_str()),
                bench_send_msgs(datas[1].as_str()),
                bench_send_msgs(datas[2].as_str()),
                bench_send_msgs(datas[3].as_str()),
            )
            .unwrap();

            let average = calc_average([first?, second?, third?, fourth?]);

            println!(
                "send messages results average warmup:\n10 msgs: {}\n100 msgs: {}\n1000 msgs: {}",
                average[0].as_secs_f64(),
                average[1].as_secs_f64(),
                average[2].as_secs_f64(),
            );

            let mut average = [Duration::ZERO; 3];

            for _ in 0..10 {
                let (first, second, third, fourth) = tokio::try_join!(
                    bench_send_msgs(datas[0].as_str()),
                    bench_send_msgs(datas[1].as_str()),
                    bench_send_msgs(datas[2].as_str()),
                    bench_send_msgs(datas[3].as_str()),
                )
                .unwrap();

                average = calc_average([average, first?, second?, third?, fourth?]);
            }

            println!(
                "send messages results average:\n10 msgs: {} secs\n100 msgs: {} secs\n1000 msgs: {} secs",
                average[0].as_secs_f64(),
                average[1].as_secs_f64(),
                average[2].as_secs_f64(),
            );
        }
        "smoketest" => {
            let run = bench_many_clients().await?;
            println!(
                "smoketest with 1000 clients x 1000 messages -> 1000 different guild/channel: took {} secs",
                run.as_secs_f64(),
            );
        }
        "single_guild" => {
            let run = bench_many_clients_single_guild().await?;
            println!(
                "1000 clients x 1000 messages -> 1 guild with event streams: took {} secs",
                run.as_secs_f64()
            );
        }
        x => println!("no such test as {}", x),
    }

    Ok(())
}

fn calc_average<const N: usize, const L: usize>(arrs: [[Duration; N]; L]) -> [Duration; N] {
    let mut temp = [Duration::ZERO; N];
    for arr in arrs {
        temp.iter_mut()
            .zip(IntoIter::new(arr))
            .for_each(|(a, b)| *a += b);
    }
    temp.iter_mut().for_each(|a| *a /= L as u32);
    temp
}

async fn bench_many_clients_single_guild() -> ClientResult<Duration> {
    let mut clients = Vec::with_capacity(1000);
    let (first_client, data) = new_test_client("test1@test.org").await?;
    let invite_id = create_invite(
        &first_client,
        CreateInvite::new(InviteId::new("test").unwrap(), -1, data.guild_id),
    )
    .await?
    .name;
    let request = JoinGuildRequest { invite_id };
    let socket = first_client
        .subscribe_events(vec![EventSource::Guild(data.guild_id)])
        .await?;
    clients.push((first_client, socket));
    for i in 2..=1000 {
        let client = new_test_client(format!("test{}@test.org", i).as_str())
            .await?
            .0;
        if !get_guild_list(&client, GetGuildListRequest {})
            .await?
            .guilds
            .iter()
            .any(|g| g.guild_id == data.guild_id)
        {
            join_guild(&client, request.clone()).await?;
        }
        let socket = client
            .subscribe_events(vec![EventSource::Guild(data.guild_id)])
            .await?;
        clients.push((client, socket));
    }

    let mut handles = Vec::with_capacity(1000);
    for (client, mut socket) in clients {
        let socket = tokio::spawn(async move {
            loop {
                socket.get_event().await;
            }
        });
        let msgs = tokio::spawn(async move {
            let since = Instant::now();
            send_messages(1000, &client, data).await?;
            ClientResult::<_>::Ok(since.elapsed())
        });
        handles.push(async move {
            tokio::select! {
                _ = socket => { panic!(); },
                res = msgs => { res },
            }
        });
    }
    let run: Duration = try_join_all(handles)
        .await
        .unwrap()
        .into_iter()
        .map(|res| res.unwrap())
        .sum();
    Ok(run / 1000)
}

async fn bench_many_clients() -> ClientResult<Duration> {
    let mut clients = Vec::with_capacity(1000);
    for i in 1..=1000 {
        clients.push(new_test_client(format!("test{}@test.org", i).as_str()).await?);
    }
    let mut handles = Vec::with_capacity(1000);
    for (client, data) in clients {
        handles.push(tokio::spawn(async move {
            let since = Instant::now();
            send_messages(1000, &client, data).await?;
            ClientResult::<_>::Ok(since.elapsed())
        }));
    }
    let run: Duration = try_join_all(handles)
        .await
        .unwrap()
        .into_iter()
        .map(|res| res.unwrap())
        .sum();
    Ok(run / 1000)
}

async fn bench_send_msgs(email: impl AsRef<str>) -> Result<ClientResult<[Duration; 3]>, JoinError> {
    let (client, data) = new_test_client(email.as_ref()).await.unwrap();

    tokio::spawn(async move {
        let sent_10_msg = send_messages(10, &client, data).await?;
        let sent_100_msg = send_messages(100, &client, data).await?;
        let sent_1000_msg = send_messages(1000, &client, data).await?;
        Ok([sent_10_msg, sent_100_msg, sent_1000_msg])
    })
    .await
}

async fn send_messages(num: usize, client: &Client, data: BenchData) -> ClientResult<Duration> {
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

async fn login(client: &Client, email: &str) -> ClientResult<()> {
    client.begin_auth().await?;
    client.next_auth_step(AuthStepResponse::Initial).await?;
    client
        .next_auth_step(AuthStepResponse::login_choice())
        .await?;
    client
        .next_auth_step(AuthStepResponse::login_form(email, PASSWORD))
        .await?;
    Ok(())
}

async fn register(client: &Client, email: &str) -> ClientResult<()> {
    client.begin_auth().await?;
    client.next_auth_step(AuthStepResponse::Initial).await?;
    let register = client
        .next_auth_step(AuthStepResponse::register_choice())
        .await?;
    if let Some(Step::Form(form)) = register.and_then(|s| s.step) {
        client
            .next_auth_step(AuthStepResponse::form(
                form.fields
                    .into_iter()
                    .map(|a| match a.name.as_str() {
                        "email" => Field::String(email.to_string()),
                        "username" => Field::String("test".to_string()),
                        "password" => Field::Bytes(PASSWORD.as_bytes().to_vec()),
                        _ => panic!(),
                    })
                    .collect(),
            ))
            .await?;
    }
    Ok(())
}

async fn new_test_client(email: &str) -> ClientResult<(Client, BenchData)> {
    let client = Client::new(SERVER_ADDR.parse().unwrap(), None).await?;
    if login(&client, email).await.is_err() {
        register(&client, email).await?;
    }

    let guild_id = if let Some(entry) = get_guild_list(&client, GetGuildListRequest {})
        .await?
        .guilds
        .pop()
    {
        entry.guild_id
    } else {
        create_guild(&client, CreateGuild::new("test".to_string()))
            .await?
            .guild_id
    };
    let channel_id = get_guild_channels(&client, GuildId::new(guild_id))
        .await?
        .channels
        .pop()
        .unwrap()
        .channel_id;

    Ok((
        client,
        BenchData {
            guild_id,
            channel_id,
        },
    ))
}

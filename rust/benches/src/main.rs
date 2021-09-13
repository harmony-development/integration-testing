use std::array::IntoIter;

use harmony_rust_sdk::{
    api::{
        auth::{auth_step::Step, next_step_request::form_fields::Field},
        chat::{
            EventSource, GetGuildChannelsRequest, GetGuildListRequest, InviteId, JoinGuildRequest,
        },
        exports::hrpc::futures_util::future::try_join_all,
    },
    client::{
        api::{
            auth::*,
            chat::{guild::CreateGuild, invite::CreateInvite, message::SendMessage},
        },
        error::*,
        *,
    },
};
use rand::{Rng, SeedableRng};
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
        // Measures throughput.
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
        // Simulates 1000 clients writing 1000 messages to 1000 different guild -> channel.
        "smoketest" => {
            let run = bench_many_clients().await?;
            println!(
                "smoketest with 1000 clients x 1000 messages -> 1000 different guild/channel: took {} secs",
                run.as_secs_f64(),
            );
        }
        // Simulates 10000 clients writing in a guild, with random wait times between each message sent by each client
        // Each client sends 10 messages, and wait times can be between 200 - 1000 milliseconds. (wait times aren't included in total / average time)
        // Each client also subscribes to an event stream and processes it.
        "single_guild" => {
            let (run, dur) = bench_many_clients_single_guild().await?;
            println!(
                "took {} secs, {} secs on average",
                dur.as_secs_f64(),
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

async fn bench_many_clients_single_guild() -> ClientResult<(Duration, Duration)> {
    const COUNT: usize = 1000;

    let mut clients = Vec::with_capacity(COUNT);
    let (first_client, data) = new_test_client("test1@test.org").await?;
    let invite_id = first_client
        .call(CreateInvite::new(
            InviteId::new("test").unwrap(),
            0,
            data.guild_id,
        ))
        .await?
        .invite_id;
    let request = JoinGuildRequest { invite_id };
    let socket = first_client
        .subscribe_events(vec![EventSource::Guild(data.guild_id)])
        .await?;
    clients.push((first_client, socket));
    for i in 2..=COUNT {
        let client = new_test_client(format!("test{}@test.org", i).as_str())
            .await?
            .0;
        if !client
            .call(GetGuildListRequest {})
            .await?
            .guilds
            .iter()
            .any(|g| g.guild_id == data.guild_id)
        {
            client.call(request.clone()).await?;
        }
        let socket = client
            .subscribe_events(vec![EventSource::Guild(data.guild_id)])
            .await?;
        clients.push((client, socket));
    }

    let mut handles = Vec::with_capacity(COUNT);
    let since = Instant::now();
    for (client, mut socket) in clients {
        let socket = tokio::spawn(async move {
            loop {
                while let Some(ev) = socket.get_event().await {
                    ev.unwrap();
                }
            }
        });
        let msgs = tokio::spawn(async move { send_messages(10, &client, data, true).await });
        handles.push(async move {
            tokio::select! {
                _ = socket => { panic!(); },
                res = msgs => { res },
            }
        });
    }
    let raw_results = try_join_all(handles).await;
    let dur = since.elapsed();
    let results = raw_results
        .unwrap()
        .into_iter()
        .map(|res| {
            let (a, b) = res.unwrap();
            [a, b]
        })
        .fold([Duration::ZERO; 2], |total, arr| {
            [total[0] + arr[0], total[1] + arr[1]]
        });

    Ok((results[0] / COUNT as u32, dur - results[1] / COUNT as u32))
}

async fn bench_many_clients() -> ClientResult<Duration> {
    let mut clients = Vec::with_capacity(1000);
    for i in 1..=1000 {
        clients.push(new_test_client(format!("test{}@test.org", i).as_str()).await?);
    }
    let mut handles = Vec::with_capacity(1000);
    for (client, data) in clients {
        handles.push(tokio::spawn(async move {
            send_messages(1000, &client, data, false)
                .await
                .map(|res| res.0)
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
        let sent_10_msg = send_messages(10, &client, data, false).await?.0;
        let sent_100_msg = send_messages(100, &client, data, false).await?.0;
        let sent_1000_msg = send_messages(1000, &client, data, false).await?.0;
        Ok([sent_10_msg, sent_100_msg, sent_1000_msg])
    })
    .await
}

async fn send_messages(
    num: usize,
    client: &Client,
    data: BenchData,
    simulate_wait: bool,
) -> ClientResult<(Duration, Duration)> {
    let mut dur = Duration::ZERO;
    let mut wait_dur = Duration::ZERO;
    let mut rng = rand::rngs::SmallRng::from_entropy();
    for i in 0..num {
        let since = Instant::now();
        client
            .call(SendMessage::new(data.guild_id, data.channel_id).text(i))
            .await?;
        dur += since.elapsed();
        if simulate_wait {
            let wait = Duration::from_millis(rng.gen_range(200..=1000));
            wait_dur += wait;
            tokio::time::sleep(wait).await;
        }
    }
    Ok((dur, wait_dur))
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
    if let Some(Step::Form(form)) = register.and_then(|s| s.step).and_then(|s| s.step) {
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

    let guild_id = if let Some(entry) = client.call(GetGuildListRequest {}).await?.guilds.pop() {
        entry.guild_id
    } else {
        client
            .call(CreateGuild::new("test".to_string()))
            .await?
            .guild_id
    };
    let channel_id = client
        .call(GetGuildChannelsRequest::new(guild_id))
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

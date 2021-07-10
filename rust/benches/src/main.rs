use std::{
    array::IntoIter,
    time::{Duration, Instant},
};

use harmony_rust_sdk::client::{
    api::{auth::*, chat::message::*},
    error::*,
    *,
};
use tokio::task::JoinHandle;

const SERVER_ADDR: &str = "https://localhost:2289";
const PASSWORD: &str = "123456789Ab";

const BENCH_DATA: &str = include_str!("../bench_data");

#[derive(Copy, Clone, Default)]
struct BenchData<'a> {
    id: &'a str,
    email: &'a str,
    guild_id: u64,
    channel_id: u64,
}

#[tokio::main]
async fn main() -> ClientResult<()> {
    let mut args = std::env::args().skip(1);
    let id = args.next().unwrap();

    let data = BenchData {
        id: Box::leak(id.into_boxed_str()),
        ..Default::default()
    };

    let datas = BENCH_DATA
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let mut split = line.split_whitespace();
            (
                index + 1,
                (
                    split.next().unwrap().parse().unwrap(),
                    split.next().unwrap().parse().unwrap(),
                ),
            )
        })
        .map(|(num, (guild_id, channel_id))| BenchData {
            email: Box::leak(format!("test{}@test.org", num).into_boxed_str()),
            guild_id,
            channel_id,
            ..data
        })
        .collect::<Vec<_>>();

    let (first, second, third, fourth) = tokio::try_join!(
        bench_send_msgs(datas[0]),
        bench_send_msgs(datas[1]),
        bench_send_msgs(datas[2]),
        bench_send_msgs(datas[3]),
    )
    .unwrap();

    let average = calc_average([first?, second?, third?, fourth?]);

    println!(
        "{} send messages results average warmup:\n10 msgs: {}\n100 msgs: {}\n1000 msgs: {}\n10000 msgs: {}",
        data.id,
        average[0].as_secs_f64(),
        average[1].as_secs_f64(),
        average[2].as_secs_f64(),
        average[3].as_secs_f64()
    );

    let mut average = [Duration::ZERO; 4];

    for _ in 0..10 {
        let (first, second, third, fourth) = tokio::try_join!(
            bench_send_msgs(datas[0]),
            bench_send_msgs(datas[1]),
            bench_send_msgs(datas[2]),
            bench_send_msgs(datas[3]),
        )
        .unwrap();

        average = calc_average([average, first?, second?, third?, fourth?]);
    }

    println!(
        "{} send messages results average:\n10 msgs: {}\n100 msgs: {}\n1000 msgs: {}\n10000 msgs: {}",
        data.id,
        average[0].as_secs_f64(),
        average[1].as_secs_f64(),
        average[2].as_secs_f64(),
        average[3].as_secs_f64()
    );

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

fn bench_send_msgs(data: BenchData<'static>) -> JoinHandle<ClientResult<[Duration; 4]>> {
    tokio::spawn(async move {
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
        Ok([sent_10_msg, sent_100_msg, sent_1000_msg, sent_10000_msg])
    })
}

async fn send_messages(num: usize, data: BenchData<'static>) -> ClientResult<Duration> {
    let client = create_new_client(data.email).await?;
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

async fn create_new_client(email: &str) -> ClientResult<Client> {
    let client = Client::new(SERVER_ADDR.parse().unwrap(), None).await?;
    client.begin_auth().await?;
    client.next_auth_step(AuthStepResponse::Initial).await?;
    client
        .next_auth_step(AuthStepResponse::login_choice())
        .await?;
    client
        .next_auth_step(AuthStepResponse::login_form(email, PASSWORD))
        .await?;
    Ok(client)
}

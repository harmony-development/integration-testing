use harmony_rust_sdk::{
    api::{
        auth::*, batch::*, chat::*, emote::*, exports::hrpc::encode::encode_protobuf_message,
        harmonytypes::ItemPosition, mediaproxy::*, profile::*, Endpoint,
    },
    client::{
        api::{
            auth::*,
            chat::{
                channel::*,
                guild::{CreateGuild, UpdateGuildInformation},
                message::*,
                permissions::QueryHasPermission,
            },
            profile::{UpdateProfile, UserStatus},
            rest::{self, FileId},
        },
        error::*,
        *,
    },
};
use rand::prelude::*;
use tokio::time::Instant;
use tracing::{error, info, info_span, Instrument, Level};
use tracing_subscriber::{prelude::*, util::SubscriberInitExt, EnvFilter};

const RUNNING_IN_GH: bool = option_env!("CI").is_some();

const EMAIL: &str = "rust_sdk_test@example.com";
const PASSWORD: Option<&str> = option_env!("TESTER_PASSWORD");

const FILE_DATA: &str = "They're waiting for you Gordon, in the test chamber.";
const FILENAME: &str = "test_chamber.txt";
const CONTENT_TYPE: &str = "text/plain";
const EXTERNAL_URL: &str =
    "https://cdn.discordapp.com/attachments/855956335689728010/855957272039260210/32b13e7ff8cb6b271db2c51aa9d6bcfb94250c7a8554c3e91fc1a9b64607ee9e.png";

const INSTANT_VIEW_URL: &str = "https://duckduckgo.com/";

const SCHERZO_DATA: TestData = TestData {
    server: "https://chat.harmonyapp.io:2289",
    name_res: "https://chat.harmonyapp.io",
    guild: 18418463542574935072,
    channel: 3775933737548659938,
    file_id: "23c782a8b622282ebc24b5664beef9500fa2a5b59131fac5b757b8e86a0e8e20",
};

static mut TESTS_COMPLETE: u16 = 0;
static mut TESTS_TOTAL: u16 = 0;
static mut TOTAL_TIME: Duration = Duration::ZERO;

#[derive(Debug, Clone, Copy)]
struct TestData {
    server: &'static str,
    name_res: &'static str,
    guild: u64,
    channel: u64,
    file_id: &'static str,
}

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::from("info"));
    let logger = tracing_subscriber::fmt::layer();

    let reg = tracing_subscriber::registry().with(filter).with(logger);

    if RUNNING_IN_GH {
        reg.with(tracing_subscriber::fmt::layer().event_format(GithubActionsFormatter))
            .init()
    } else {
        reg.init()
    }

    unsafe {
        TESTS_COMPLETE = 0;
        TESTS_TOTAL = 0;
        TOTAL_TIME = Duration::ZERO;
    }
    let s = tests(SCHERZO_DATA).instrument(info_span!("scherzo")).await;
    let st = unsafe { TOTAL_TIME };

    info!(
        "Scherzo: {} tests successful, {} tests ran, completed in {} secs",
        s,
        unsafe { TESTS_TOTAL },
        st.as_secs_f64()
    );
}

async fn tests(data: TestData) -> u16 {
    {
        test(
            "name resolution",
            Client::new(data.name_res.parse().unwrap(), None),
            |_| async {},
        )
        .await;
    }

    test(
        "client connection",
        Client::new(data.server.parse().unwrap(), None),
        |client| async move {
            test(
                "client auth",
                async {
                    async fn wait_for_socket(sock: &mut AuthSocket) {
                        let fut = async move {
                            loop {
                                if let Ok(Some(a)) = sock.get_step().await {
                                    tracing::info!("auth socket reply: {:?}", a);
                                    break;
                                }
                            }
                        };
                        let dur = Duration::from_secs(5);

                        tokio::time::timeout(dur, fut)
                            .await
                            .expect("did not receive auth step from stream");
                    }

                    let login = async {
                        client.begin_auth().await?;
                        let mut auth_sock = client.auth_stream().await?;

                        client.next_auth_step(AuthStepResponse::Initial).await?;
                        wait_for_socket(&mut auth_sock).await;

                        client
                            .next_auth_step(AuthStepResponse::login_choice())
                            .await?;
                        wait_for_socket(&mut auth_sock).await;

                        client
                            .next_auth_step(AuthStepResponse::login_form(
                                EMAIL,
                                PASSWORD.expect("no tester password?"),
                            ))
                            .await?;
                        wait_for_socket(&mut auth_sock).await;

                        ClientResult::Ok(())
                    };

                    if login.await.is_err() {
                        client.begin_auth().await?;
                        let mut auth_sock = client.auth_stream().await?;

                        client.next_auth_step(AuthStepResponse::Initial).await?;
                        wait_for_socket(&mut auth_sock).await;

                        client
                            .next_auth_step(AuthStepResponse::register_choice())
                            .await?;
                        wait_for_socket(&mut auth_sock).await;

                        client
                            .next_auth_step(AuthStepResponse::register_form(
                                EMAIL,
                                "rust_sdk_test",
                                PASSWORD.expect("no tester password?"),
                            ))
                            .await?;
                        wait_for_socket(&mut auth_sock).await;
                    }

                    ClientResult::Ok(())
                },
                |_a| async {
                    check!(client.auth_status().is_authenticated(), true);

                    test(
                        "check logged in",
                        client.call(CheckLoggedInRequest::new()),
                        |_| async {},
                    )
                    .await;
                    let user_id = client.auth_status().session().unwrap().user_id;

                    test_no_hand(
                        "profile update",
                        client.call(UpdateProfile::default().with_new_status(UserStatus::Online)),
                    )
                    .await;

                    test_no_hand(
                        "preview guild",
                        client.call(PreviewGuildRequest::new("harmony".to_string())),
                    )
                    .await;

                    test(
                        "get guild list",
                        client.call(GetGuildListRequest {}),
                        |response| async move {
                            check!(response.guilds.len(), 1);
                        },
                    )
                    .await;

                    test_no_hand(
                        "get guild roles",
                        client.call(GetGuildRolesRequest::new(data.guild)),
                    )
                    .await;

                    test(
                        "get guild members",
                        client.call(GetGuildMembersRequest::new(data.guild)),
                        |response| async {
                            check!(response.members.len(), 1);

                            test_no_hand(
                                "get profile",
                                client.call(GetProfileRequest::new(
                                    *response
                                        .members
                                        .first()
                                        .expect("expected at least one user in guild"),
                                )),
                            )
                            .await;

                            test_no_hand("get user bulk", {
                                let requests = response
                                    .members
                                    .into_iter()
                                    .map(|user_id| {
                                        let req = GetProfileRequest::new(user_id);
                                        let data = encode_protobuf_message(&req);
                                        data.freeze()
                                    })
                                    .collect();
                                client.call(BatchSameRequest::new(
                                    GetProfileRequest::ENDPOINT_PATH.to_string(),
                                    requests,
                                ))
                            })
                            .await;
                        },
                    )
                    .await;

                    test_no_hand("get emote packs", client.call(GetEmotePacksRequest {})).await;

                    test_no_hand(
                        "get guild channels",
                        client.call(GetGuildChannelsRequest::new(data.guild)),
                    )
                    .await;

                    test_no_hand(
                        "typing",
                        client.call(TypingRequest::new(data.guild, data.channel)),
                    )
                    .await;

                    let current_time = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
                    let msg = format!("test at {}", current_time);
                    test_no_hand(
                        "send message",
                        client.call(SendMessage::new(data.guild, data.channel).text(&msg)),
                    )
                    .await;

                    test(
                        "get channel messages",
                        client.call(GetChannelMessages::new(data.guild, data.channel)),
                        |response| {
                            let client = client.clone();
                            async move {
                                let our_msg = response.messages.first().unwrap();
                                let (message_id, message) =
                                    (our_msg.message_id, our_msg.message.as_ref().unwrap());
                                check!(message.text(), Some(msg.as_str()));

                                let new_content = rand::thread_rng()
                                    .sample_iter(rand::distributions::Alphanumeric)
                                    .take(16)
                                    .map(|c| c as char)
                                    .collect::<String>();

                                test(
                                    "edit message",
                                    client.call(UpdateMessageTextRequest {
                                        guild_id: data.guild,
                                        channel_id: data.channel,
                                        message_id,
                                        new_content: Some(
                                            FormattedText::default().with_text(new_content.clone()),
                                        ),
                                    }),
                                    |_| async {
                                        test(
                                            "compare get message",
                                            client.call(GetMessageRequest {
                                                guild_id: data.guild,
                                                channel_id: data.channel,
                                                message_id,
                                            }),
                                            |response| async move {
                                                check!(
                                                    response.message.as_ref().unwrap().text(),
                                                    Some(new_content.as_str())
                                                );
                                            },
                                        )
                                        .await;
                                    },
                                )
                                .await;
                            }
                        },
                    )
                    .await;

                    test_no_hand(
                        "instant view",
                        client.call(InstantViewRequest::new(INSTANT_VIEW_URL.to_string())),
                    )
                    .await;

                    test_no_hand(
                        "can instant view",
                        client.call(CanInstantViewRequest::new(INSTANT_VIEW_URL.to_string())),
                    )
                    .await;

                    test_no_hand(
                        "fetch link metadata",
                        client.call(FetchLinkMetadataRequest::new(INSTANT_VIEW_URL.to_string())),
                    )
                    .await;

                    test(
                        "upload media",
                        rest::upload(
                            &client,
                            FILENAME.to_string(),
                            CONTENT_TYPE.to_string(),
                            FILE_DATA.as_bytes().to_vec(),
                        ),
                        |response| async {
                            test_no_hand("upload response id", response.text()).await;
                        },
                    )
                    .await;

                    test(
                        "download media",
                        rest::download(&client, FileId::Id(data.file_id.to_string())),
                        |response| async {
                            let content_type = response
                                .headers()
                                .get("Content-Type")
                                .map(|c| c.to_str().ok().map(|c| c.to_string()))
                                .flatten();

                            if let Some(content_type) = content_type {
                                test(
                                    "download response text",
                                    response.text(),
                                    |response| async move {
                                        check!(response.as_str(), FILE_DATA);
                                    },
                                )
                                .await;
                                check!(content_type.as_str(), CONTENT_TYPE);
                            }
                        },
                    )
                    .await;

                    test(
                        "download external file",
                        rest::download(&client, FileId::External(EXTERNAL_URL.parse().unwrap())),
                        |response| async {
                            if response.bytes().await.is_err() {
                                tracing::error!("failed to download external file bytes");
                            } else {
                                tracing::info!("successfully downloaded external file bytes");
                            }
                        },
                    )
                    .await;

                    test(
                        "get guild channels",
                        client.call(GetGuildChannelsRequest::new(data.guild)),
                        |response| async move {
                            check!(response.channels.len(), 1);
                        },
                    )
                    .await;

                    test(
                        "create channel",
                        client.call(CreateChannel::new(
                            data.guild,
                            "test".to_string(),
                            ItemPosition::new_after(data.channel),
                        )),
                        |response| {
                            let client = client.clone();
                            async move {
                                test(
                                    "get channels compare new",
                                    client.call(GetGuildChannelsRequest::new(data.guild)),
                                    |response| async move {
                                        check!(response.channels.len(), 2);
                                    },
                                )
                                .await;
                                test(
                                    "delete channel",
                                    client
                                        .call(DeleteChannel::new(data.guild, response.channel_id)),
                                    |_| async {
                                        test(
                                            "get channels compare delete",
                                            client.call(GetGuildChannelsRequest::new(data.guild)),
                                            |response| async move {
                                                check!(response.channels.len(), 1);
                                            },
                                        )
                                        .await;
                                    },
                                )
                                .await;
                            }
                        },
                    )
                    .await;

                    test_no_hand(
                        "get guild information",
                        client.call(GetGuildRequest::new(data.guild)),
                    )
                    .await;

                    let new_name = rand::thread_rng()
                        .sample_iter(rand::distributions::Alphanumeric)
                        .take(16)
                        .map(|c| c as char)
                        .collect::<String>();

                    test(
                        "update guild information",
                        client.call(
                            UpdateGuildInformation::new(data.guild)
                                .with_new_guild_name(new_name.clone()),
                        ),
                        {
                            let client = client.clone();
                            move |_| async move {
                                test(
                                    "compare new info",
                                    client.call(GetGuildRequest::new(data.guild)),
                                    |response| async move {
                                        check!(response.guild.as_ref().unwrap().name, new_name);
                                    },
                                )
                                .await;
                            }
                        },
                    )
                    .await;

                    test(
                        "create guild",
                        client.call(CreateGuild::new("test".to_string())),
                        |response| {
                            let client = client.clone();
                            async move {
                                test_no_hand(
                                    "delete guild",
                                    client.call(DeleteGuildRequest::new(response.guild_id)),
                                )
                                .await;
                            }
                        },
                    )
                    .await;

                    test(
                        "query has permission",
                        client.call(
                            QueryHasPermission::new(data.guild, "messages.send".to_string())
                                .with_channel_id(data.channel),
                        ),
                        |response| async move {
                            check!(response.ok, true);
                        },
                    )
                    .await;

                    test(
                        "set profile offline",
                        client.call(
                            UpdateProfile::default()
                                .with_new_status(UserStatus::OfflineUnspecified),
                        ),
                        |_| async {
                            test(
                                "compare profile status",
                                client.call(GetProfileRequest::new(user_id)),
                                |response| async move {
                                    check!(
                                        response.profile.as_ref().unwrap().user_status,
                                        i32::from(UserStatus::OfflineUnspecified)
                                    );
                                },
                            )
                            .await;
                        },
                    )
                    .await;

                    test(
                        "set profile bot",
                        client.call(UpdateProfile::default().with_new_is_bot(true)),
                        |_| async {
                            test(
                                "compare profile bot",
                                client.call(GetProfileRequest::new(user_id)),
                                |response| async move {
                                    check!(response.profile.as_ref().unwrap().is_bot, true);
                                },
                            )
                            .await;
                        },
                    )
                    .await;
                },
            )
            .await;
        },
    )
    .await;

    unsafe { TESTS_COMPLETE }
}

async fn test<Fut, HandFut, Hand, Out, Err>(name: &'static str, res: Fut, hand: Hand)
where
    Err: std::error::Error,
    Out: Debug,
    Fut: Future<Output = Result<Out, Err>>,
    HandFut: Future<Output = ()>,
    Hand: FnOnce(Out) -> HandFut,
{
    info!("Testing {}...", name);
    unsafe {
        TESTS_TOTAL += 1;
    }
    let ins = Instant::now();
    async {
        match res.await {
            Ok(val) => {
                let time_passed = ins.elapsed();
                unsafe {
                    TOTAL_TIME += time_passed;
                }
                info!("successful in {} ns", time_passed.as_nanos());
                info!("response: {:?}", val);
                unsafe {
                    TESTS_COMPLETE += 1;
                }
                hand(val).await
            }
            Err(err) => error!("error occured: {}", err),
        }
    }
    .await
}

async fn test_no_hand<Fut, Out, Err>(name: &'static str, res: Fut)
where
    Err: std::error::Error,
    Out: Debug,
    Fut: Future<Output = Result<Out, Err>>,
{
    test(name, res, |_| async {}).await
}

#[macro_export]
macro_rules! check {
    ($res:expr, $res2:expr) => {
        unsafe {
            TESTS_TOTAL += 1;
        }
        if $res != $res2 {
            error!("check unsuccessful: {:?} != {:?}", $res, $res2);
        } else {
            unsafe {
                TESTS_COMPLETE += 1;
            }
        }
    };
}

use std::{
    fmt::{self, Debug},
    future::Future,
    time::Duration,
};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::{
    format::Writer as TracingWriter, FmtContext, FormatEvent, FormatFields,
};
use tracing_subscriber::registry::LookupSpan;

struct GithubActionsFormatter;

impl<S, N> FormatEvent<S, N> for GithubActionsFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: TracingWriter<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        let level = metadata.level();

        if let Some(lvl) = level
            .eq(&Level::WARN)
            .then(|| "warning")
            .or_else(|| level.eq(&Level::ERROR).then(|| "error"))
        {
            let file = metadata.file();
            let line = metadata.line();

            write!(&mut writer, "::{}", lvl)?;
            if let Some(file_name) = file {
                write!(&mut writer, " file={}", file_name)?;
            }
            if let Some(line) = line {
                if file.is_some() {
                    write!(&mut writer, ",line={}", line)?;
                } else {
                    write!(&mut writer, " line={}", line)?;
                }
            }
            write!(&mut writer, "::")?;

            // Write spans and fields of each span
            ctx.visit_spans(|span| match span.name() {
                "client connection" | "client auth" => Ok(()),
                _ => write!(&mut writer, "/{}", span.name().replace(' ', "_")),
            })?;

            write!(&mut writer, ": ")?;

            ctx.field_format().format_fields(writer, event)?;
        }

        Ok(())
    }
}

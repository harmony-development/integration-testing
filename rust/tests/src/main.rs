use harmony_rust_sdk::{
    api::{
        auth::*, batch::*, chat::*, emote::*, exports::hrpc::encode_protobuf_message,
        exports::hrpc::tracing, mediaproxy::*, profile::*,
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
            *,
        },
        error::*,
        *,
    },
};
use rand::prelude::*;
use rest::FileId;
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
    guild: 9496763902128586438,
    channel: 6751423531778516907,
    file_id: "agfR1jmjclto9OoGwmlNvM95jBLxMi0zTiu5ilTaj095Cap2QFX2OlQyfB66iG2W",
};

static mut TESTS_COMPLETE: u16 = 0;
static mut TESTS_TOTAL: u16 = 0;
static mut TOTAL_TIME: Duration = Duration::ZERO;

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
        "Scherzo: {} out of {} tests successful, completed in {} secs",
        s,
        unsafe { TESTS_TOTAL },
        st.as_secs_f64()
    );
}

async fn tests(data: TestData) -> u16 {
    {
        test! {
            "name resolution",
            Client::new(data.name_res.parse().unwrap(), None),
        }
    }

    test! {
        "client connection",
        Client::new(data.server.parse().unwrap(), None),
        |client| {
            test! {
                "client auth",
                async {
                    client.begin_auth().await?;
                    client.next_auth_step(AuthStepResponse::Initial).await?;
                    client
                        .next_auth_step(AuthStepResponse::login_choice())
                        .await?;
                    client
                        .next_auth_step(AuthStepResponse::login_form(
                            EMAIL,
                            PASSWORD.expect("no tester password?"),
                        ))
                        .await?;
                    ClientResult::Ok(())
                },
                |_a| {
                    check!(client.auth_status().is_authenticated(), true);

                    test! {
                        "check logged in",
                        client.call(CheckLoggedInRequest::new()),
                    }
                    let user_id = client.auth_status().session().unwrap().user_id;

                    test! {
                        "profile update",
                        client.call(
                            UpdateProfile::default().with_new_status(UserStatus::Online),
                        ),
                    }

                    test! {
                        "preview guild",
                        client.call(PreviewGuildRequest::new("harmony".to_string())),
                    }

                    test! {
                        "get guild list",
                        client.call(GetGuildListRequest {}),
                        |response| {
                            check!(response.guilds.len(), 1);
                        }
                    }

                    test! {
                        "get guild roles",
                        client.call(GetGuildRolesRequest::new(data.guild)),
                    }

                    test! {
                        "get guild members",
                        client.call(GetGuildMembersRequest::new(data.guild)),
                        |response| {
                            check!(response.members.len(), 1);

                            test! {
                                "get profile",
                                client.call(
                                    GetProfileRequest::new(
                                        *response
                                            .members
                                            .first()
                                            .expect("expected at least one user in guild"),
                                    ),
                                ),
                            }

                            test! {
                                "get user bulk",
                                {
                                    let requests = response.members.into_iter().map(|user_id| {
                                        let req = GetProfileRequest::new(user_id);
                                        let data = encode_protobuf_message(req);
                                        data.to_vec()
                                    }).collect();
                                    client.call(BatchSameRequest::new(GetProfileRequest::ENDPOINT_PATH.to_string(), requests))
                                },
                            }
                        }
                    }

                    test! {
                        "get emote packs",
                        client.call(GetEmotePacksRequest {}),
                    }

                    test! {
                        "get guild channels",
                        client.call(GetGuildChannelsRequest::new(data.guild)),
                    }

                    test! {
                        "typing",
                        client.call(TypingRequest::new(data.guild, data.channel)),
                    }

                    let current_time = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
                    let msg = format!("test at {}", current_time);
                    test! {
                        "send message",
                        client.call(
                            SendMessage::new(data.guild, data.channel).text(&msg),
                        ),
                    }

                    test! {
                        "get channel messages",
                        client.call(GetChannelMessages::new(data.guild, data.channel)),
                        |response| {
                            let our_msg = response.messages.first().unwrap();
                            let (message_id, message) = (our_msg.message_id, our_msg.message.as_ref().unwrap());
                            check!(message.text(), Some(msg.as_str()));

                            let new_content = rand::thread_rng()
                                .sample_iter(rand::distributions::Alphanumeric)
                                .take(16)
                                .map(|c| c as char)
                                .collect::<String>();

                            test! {
                                "edit message",
                                client.call(
                                    UpdateMessageTextRequest {
                                        guild_id: data.guild,
                                        channel_id: data.channel,
                                        message_id,
                                        new_content: Some(FormattedText::default().with_text(new_content.clone())),
                                    },
                                ),
                                |response| {
                                    test! {
                                        "compare get message",
                                        client.call(GetMessageRequest {
                                            guild_id: data.guild,
                                            channel_id: data.channel,
                                            message_id,
                                        }),
                                        |response| {
                                            check!(response.message.as_ref().unwrap().text(), Some(new_content.as_str()));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    test! {
                        "instant view",
                        client.call(InstantViewRequest::new(INSTANT_VIEW_URL.to_string())),
                    }

                    test! {
                        "can instant view",
                        client.call(CanInstantViewRequest::new(INSTANT_VIEW_URL.to_string())),
                    }

                    test! {
                        "fetch link metadata",
                        client.call(FetchLinkMetadataRequest::new(INSTANT_VIEW_URL.to_string())),
                    }

                    test! {
                        "upload media",
                        rest::upload(
                            &client,
                            FILENAME.to_string(),
                            CONTENT_TYPE.to_string(),
                            FILE_DATA.as_bytes().to_vec(),
                        ),
                        |response| {
                            test! {
                                "upload response id",
                                response.text(),
                            }
                        }
                    }

                    test! {
                        "download media",
                        rest::download(&client, FileId::Id(data.file_id.to_string())),
                        |response| {

                            let content_type = response
                            .headers()
                            .get("Content-Type")
                            .map(|c| c.to_str().ok().map(|c| c.to_string()))
                            .flatten();

                            if let Some(content_type) = content_type {
                                test! {
                                    "download response text",
                                    response.text(),
                                    |response| {
                                        check!(response.as_str(), FILE_DATA);
                                    }
                                }
                                check!(content_type.as_str(), CONTENT_TYPE);
                            }
                        }
                    }

                    test! {
                        "download external file",
                        rest::download(&client, FileId::External(EXTERNAL_URL.parse().unwrap())),
                        |response| {
                            if response.bytes().await.is_err() {
                                tracing::error!("failed to download external file bytes");
                            } else {
                                tracing::info!("successfully downloaded external file bytes");
                            }
                        }
                    }

                    test! {
                        "get guild channels",
                        client.call(GetGuildChannelsRequest::new(data.guild)),
                        |response| {
                            check!(response.channels.len(), 1);
                        }
                    }

                    test! {
                        "create channel",
                        client.call(
                            CreateChannel::new(data.guild, "test".to_string(), Place::bottom(data.channel)),
                        ),
                        |response| {
                            test! {
                                "get channels compare new",
                                client.call(GetGuildChannelsRequest::new(data.guild)),
                                |response| {
                                    check!(response.channels.len(), 2);
                                }
                            }
                            test! {
                                "delete channel",
                                client.call(DeleteChannel::new(data.guild, response.channel_id)),
                                |response| {
                                    test! {
                                        "get channels compare delete",
                                        client.call(GetGuildChannelsRequest::new(data.guild)),
                                        |response| {
                                            check!(response.channels.len(), 1);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    test! {
                        "get guild information",
                        client.call(GetGuildRequest::new(data.guild)),
                    }

                    let new_name = rand::thread_rng()
                        .sample_iter(rand::distributions::Alphanumeric)
                        .take(16)
                        .map(|c| c as char)
                        .collect::<String>();

                    test! {
                        "update guild information",
                        client.call(
                            UpdateGuildInformation::new(data.guild).with_new_guild_name(new_name.clone())
                        ),
                        |response| {
                            test! {
                                "compare new info",
                                client.call(GetGuildRequest::new(data.guild)),
                                |response| {
                                    check!(response.guild.as_ref().unwrap().name, new_name);
                                }
                            }
                        }
                    }

                    test! {
                        "create guild",
                        client.call(CreateGuild::new("test".to_string())),
                        |response| {
                            test! {
                                "delete guild",
                                client.call(DeleteGuildRequest::new(response.guild_id)),
                            }
                        }
                    }

                    test! {
                        "query has permission",
                        client.call(
                            QueryHasPermission::new(data.guild, "messages.send".to_string()).with_channel_id(data.channel),
                        ),
                        |response| {
                            check!(response.ok, true);
                        }
                    }

                    test! {
                        "set profile offline",
                        client.call(
                            UpdateProfile::default().with_new_status(UserStatus::OfflineUnspecified),
                        ),
                        |response| {
                            test! {
                                "compare profile status",
                                client.call(GetProfileRequest::new(user_id)),
                                |response| {
                                    check!(response.profile.as_ref().unwrap().user_status, i32::from(UserStatus::OfflineUnspecified));
                                }
                            }
                        }
                    }

                    test! {
                        "set profile bot",
                        client.call(
                            UpdateProfile::default().with_new_is_bot(true),
                        ),
                        |response| {
                            test! {
                                "compare profile bot",
                                client.call(GetProfileRequest::new(user_id)),
                                |response| {
                                    check!(response.profile.as_ref().unwrap().is_bot, true);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    unsafe { TESTS_COMPLETE }
}

#[macro_export]
macro_rules! test {
    ($name:expr, $res:expr,) => {
        test!($name, $res, |_a| ());
    };
    {
        $name:expr,
        $res:expr,
        |$val:ident| $sub:expr
    } => {
        info!("Testing {}...", $name);
        unsafe { TESTS_TOTAL += 1; }
        let span = info_span!($name);
        let ins = Instant::now();
        async {
            match $res.await {
                Ok($val) => {
                    let time_passed = ins.elapsed();
                    unsafe { TOTAL_TIME += time_passed; }
                    info!("successful in {} ns", time_passed.as_nanos());
                    info!("response: {:?}", $val);
                    unsafe { TESTS_COMPLETE += 1; }
                    $sub
                },
                Err(err) => error!("error occured: {}", err),
            }
        }.instrument(span).await
    };
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

use std::{fmt, time::Duration};
use tracing::{Event, Subscriber};
use tracing_subscriber::fmt::{FmtContext, FormatEvent, FormatFields};
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
        writer: &mut dyn fmt::Write,
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

            write!(writer, "::{}", lvl)?;
            if let Some(file_name) = file {
                write!(writer, " file={}", file_name)?;
            }
            if let Some(line) = line {
                if file.is_some() {
                    write!(writer, ",line={}", line)?;
                } else {
                    write!(writer, " line={}", line)?;
                }
            }
            write!(writer, "::")?;

            // Write spans and fields of each span
            ctx.visit_spans(|span| match span.name() {
                "client connection" | "client auth" => Ok(()),
                _ => write!(writer, "/{}", span.name().replace(' ', "_")),
            })?;

            write!(writer, ": ")?;

            ctx.field_format().format_fields(writer, event)?;

            writeln!(writer)?;
        }

        Ok(())
    }
}

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

const LEGATO_DATA: TestData = TestData {
    server: "https://chat.harmonyapp.io:2289",
    name_res: "https://chat.harmonyapp.io",
    guild: 2721664628324040709,
    channel: 2721664628324106245,
    file_id: "403cb46c-49cf-4ae1-b876-f38eb26accb0",
};

const SCHERZO_DATA: TestData = TestData {
    server: "https://scherzo.harmonyapp.io:2289",
    name_res: "https://scherzo.harmonyapp.io",
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
    let l = tests(LEGATO_DATA).instrument(info_span!("legato")).await;
    let lt = unsafe { TOTAL_TIME };

    unsafe {
        TESTS_COMPLETE = 0;
        TESTS_TOTAL = 0;
        TOTAL_TIME = Duration::ZERO;
    }
    let s = tests(SCHERZO_DATA).instrument(info_span!("scherzo")).await;
    let st = unsafe { TOTAL_TIME };

    info!(
        "Legato: {} out of {} tests successful, completed in {} secs",
        l,
        unsafe { TESTS_TOTAL },
        lt.as_secs_f64()
    );
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
                        auth::check_logged_in(&client, ()),
                    }
                    let user_id = client.auth_status().session().unwrap().user_id;

                    let mut events = {
                        test! {
                            "stream events",
                            chat::stream_events(&client),
                        }
                    };

                    test! {
                        "profile update",
                        profile::profile_update(
                            &client,
                            ProfileUpdate::default().new_status(harmonytypes::UserStatus::OnlineUnspecified),
                        ),
                    }

                    test! {
                        "preview guild",
                        guild::preview_guild(&client, invite::InviteId::new("harmony").unwrap()),
                    }

                    test! {
                        "get guild list",
                        guild::get_guild_list(&client, GetGuildListRequest {}),
                        |response| {
                            check!(response.guilds.len(), 1);
                        }
                    }

                    test! {
                        "get guild roles",
                        permissions::get_guild_roles(&client, GuildId::new(data.guild)),
                    }

                    test! {
                        "get guild members",
                        guild::get_guild_members(&client, GuildId::new(data.guild)),
                        |response| {
                            check!(response.members.len(), 1);

                            test! {
                                "get user",
                                profile::get_user(
                                    &client,
                                    UserId::new(
                                        *response
                                            .members
                                            .first()
                                            .expect("expected at least one user in guild"),
                                    ),
                                ),
                            }

                            test! {
                                "get user bulk",
                                profile::get_user_bulk(&client, response.members),
                            }
                        }
                    }

                    test! {
                        "get emote packs",
                        emote::get_emote_packs(&client, GetEmotePacksRequest {}),
                    }

                    test! {
                        "get guild channels",
                        channel::get_guild_channels(&client, GuildId::new(data.guild)),
                    }

                    test! {
                        "typing",
                        typing(&client, Typing::new(data.guild, data.channel)),
                    }

                    let current_time = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
                    let msg = format!("test at {}", current_time);
                    test! {
                        "send message",
                        message::send_message(
                            &client,
                            SendMessage::new(data.guild, data.channel).text(&msg),
                        ),
                    }

                    test! {
                        "get channel messages",
                        channel::get_channel_messages(&client, GetChannelMessages::new(data.guild, data.channel)),
                        |response| {
                            let our_msg = response.messages.first().unwrap();
                            check!(our_msg.text(), Some(msg.as_str()));

                            let new_content = rand::thread_rng()
                                .sample_iter(rand::distributions::Alphanumeric)
                                .take(16)
                                .map(|c| c as char)
                                .collect::<String>();

                            test! {
                                "edit message",
                                message::update_message_text(
                                    &client,
                                    UpdateMessageTextRequest {
                                        guild_id: data.guild,
                                        channel_id: data.channel,
                                        message_id: our_msg.message_id,
                                        new_content: new_content.clone(),
                                    },
                                ),
                                |response| {
                                    test! {
                                        "compare get message",
                                        message::get_message(&client, GetMessageRequest {
                                            guild_id: data.guild,
                                            channel_id: data.channel,
                                            message_id: our_msg.message_id,
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
                        mediaproxy::instant_view(&client, INSTANT_VIEW_URL.parse::<Url>().unwrap()),
                    }

                    test! {
                        "can instant view",
                        mediaproxy::can_instant_view(&client, INSTANT_VIEW_URL.parse::<Url>().unwrap()),
                    }

                    test! {
                        "fetch link metadata",
                        mediaproxy::fetch_link_metadata(&client, INSTANT_VIEW_URL.parse::<Url>().unwrap()),
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
                        channel::get_guild_channels(&client, GuildId::new(data.guild)),
                        |response| {
                            check!(response.channels.len(), 1);
                        }
                    }

                    test! {
                        "create channel",
                        channel::create_channel(
                            &client,
                            CreateChannel::new(data.guild, "test".to_string(), Place::bottom(data.channel)),
                        ),
                        |response| {
                            test! {
                                "get channels compare new",
                                channel::get_guild_channels(&client, GuildId::new(data.guild)),
                                |response| {
                                    check!(response.channels.len(), 2);
                                }
                            }
                            test! {
                                "delete channel",
                                channel::delete_channel(&client, DeleteChannel::new(data.guild, response.channel_id)),
                                |response| {
                                    test! {
                                        "get channels compare delete",
                                        channel::get_guild_channels(&client, GuildId::new(data.guild)),
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
                        guild::get_guild(&client, GuildId::new(data.guild)),
                    }

                    let new_name = rand::thread_rng()
                        .sample_iter(rand::distributions::Alphanumeric)
                        .take(16)
                        .map(|c| c as char)
                        .collect::<String>();

                    test! {
                        "update guild information",
                        guild::update_guild_information(
                            &client,
                            UpdateGuildInformation::new(data.guild).new_guild_name(new_name.clone())
                        ),
                        |response| {
                            test! {
                                "compare new info",
                                guild::get_guild(&client, GuildId::new(data.guild)),
                                |response| {
                                    check!(response.guild_name, new_name);
                                }
                            }
                        }
                    }

                    test! {
                        "create guild",
                        guild::create_guild(&client, CreateGuild::new("test".to_string())),
                        |response| {
                            test! {
                                "delete guild",
                                guild::delete_guild(&client, GuildId::new(response.guild_id)),
                            }
                        }
                    }

                    test! {
                        "query has permission",
                        permissions::query_has_permission(
                            &client,
                            QueryPermissions::new(data.guild, "messages.send".to_string()).channel_id(data.channel),
                        ),
                        |response| {
                            check!(response.ok, true);
                        }
                    }

                    test! {
                        "set profile offline",
                        profile::profile_update(
                            &client,
                            ProfileUpdate::default().new_status(harmonytypes::UserStatus::Offline),
                        ),
                        |response| {
                            test! {
                                "compare profile status",
                                profile::get_user(&client, UserId::new(user_id)),
                                |response| {
                                    check!(response.user_status, i32::from(harmonytypes::UserStatus::Offline));
                                }
                            }
                        }
                    }

                    test! {
                        "set profile bot",
                        profile::profile_update(
                            &client,
                            ProfileUpdate::default().new_is_bot(true),
                        ),
                        |response| {
                            test! {
                                "compare profile bot",
                                profile::get_user(&client, UserId::new(user_id)),
                                |response| {
                                    check!(response.is_bot, true);
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

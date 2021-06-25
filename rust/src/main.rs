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

    let ins = Instant::now();
    let l = tests(LEGATO_DATA).instrument(info_span!("legato")).await;
    let lt = ins.elapsed();

    let ins = Instant::now();
    let s = tests(SCHERZO_DATA).instrument(info_span!("scherzo")).await;
    let st = ins.elapsed();

    info!(
        "Legato: {} out of 41 tests successful, completed in {} secs",
        l,
        lt.as_secs_f64()
    );
    info!(
        "Scherzo: {} out of 41 tests successful, completed in {} secs",
        s,
        st.as_secs_f64()
    );
}

async fn tests(data: TestData) -> u16 {
    let mut tests_complete = 0;

    {
        test! {
            "name resolution",
            Client::new(data.name_res.parse().unwrap(), None),
            |_a| {
                tests_complete += 1;
            }
        }
    }

    test! {
        "client connection",
        Client::new(data.server.parse().unwrap(), None),
        |client| {
            info!("Created client");
            tests_complete += 1;

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
                    info!("Logged in");
                    tests_complete += 1;

                    test! {
                        "check logged in",
                        auth::check_logged_in(&client, ()),
                        |_a| {
                            info!("Logged in");
                            tests_complete += 1;
                        }
                    }
                    let user_id = client.auth_status().session().unwrap().user_id;

                    test! {
                        "stream events",
                        chat::stream_events(&client),
                        |_a| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "profile update",
                        profile::profile_update(
                            &client,
                            ProfileUpdate::default().new_status(harmonytypes::UserStatus::OnlineUnspecified),
                        ),
                        |_a| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "preview guild",
                        guild::preview_guild(&client, invite::InviteId::new("harmony").unwrap()),
                        |response| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "get guild list",
                        guild::get_guild_list(&client, GetGuildListRequest {}),
                        |response| {
                            check!(response.guilds.len(), 1);
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "get guild roles",
                        permissions::get_guild_roles(&client, GuildId::new(data.guild)),
                        |response| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "get guild members",
                        guild::get_guild_members(&client, GuildId::new(data.guild)),
                        |response| {
                            check!(response.members.len(), 1);
                            tests_complete += 1;

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
                                |response| {
                                    tests_complete += 1;
                                }
                            }

                            test! {
                                "get user bulk",
                                profile::get_user_bulk(&client, response.members),
                                |response| {
                                    tests_complete += 1;
                                }
                            }
                        }
                    }

                    test! {
                        "get emote packs",
                        emote::get_emote_packs(&client, GetEmotePacksRequest {}),
                        |response| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "get guild channels",
                        channel::get_guild_channels(&client, GuildId::new(data.guild)),
                        |response| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "typing",
                        typing(&client, Typing::new(data.guild, data.channel)),
                        |response| {
                            info!("Notified the server that we are typing");
                            tests_complete += 1;
                        }
                    }

                    let current_time = std::time::UNIX_EPOCH.elapsed().unwrap().as_secs();
                    let msg = format!("test at {}", current_time);
                    test! {
                        "send message",
                        message::send_message(
                            &client,
                            SendMessage::new(data.guild, data.channel).text(&msg),
                        ),
                        |response| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "get channel messages",
                        channel::get_channel_messages(&client, GetChannelMessages::new(data.guild, data.channel)),
                        |response| {
                            let our_msg = response.messages.first().unwrap();
                            check!(our_msg.text(), Some(msg.as_str()));
                            tests_complete += 1;


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
                                    tests_complete += 1;

                                    test! {
                                        "compare get message",
                                        message::get_message(&client, GetMessageRequest {
                                            guild_id: data.guild,
                                            channel_id: data.channel,
                                            message_id: our_msg.message_id,
                                        }),
                                        |response| {
                                            check!(response.message.as_ref().unwrap().text(), Some(new_content.as_str()));
                                            tests_complete += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    test! {
                        "instant view",
                        mediaproxy::instant_view(&client, INSTANT_VIEW_URL.parse::<Url>().unwrap()),
                        |response| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "can instant view",
                        mediaproxy::can_instant_view(&client, INSTANT_VIEW_URL.parse::<Url>().unwrap()),
                        |response| {
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "fetch link metadata",
                        mediaproxy::fetch_link_metadata(&client, INSTANT_VIEW_URL.parse::<Url>().unwrap()),
                        |response| {
                            tests_complete += 1;
                        }
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
                            tests_complete += 1;

                            test! {
                                "upload response id",
                                response.text(),
                                |response| {
                                    tests_complete += 1;
                                }
                            }
                        }
                    }

                    test! {
                        "download media",
                        rest::download(&client, FileId::Id(data.file_id.to_string())),
                        |response| {
                            tests_complete += 1;

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
                                        tests_complete += 1;
                                    }
                                }
                                check!(content_type.as_str(), CONTENT_TYPE);
                                tests_complete += 1;
                            }
                        }
                    }

                    test! {
                        "download external file",
                        rest::download(&client, FileId::External(EXTERNAL_URL.parse().unwrap())),
                        |response| {
                            tests_complete += 1;
                            if response.bytes().await.is_err() {
                                tracing::error!("failed to download external file bytes");
                            } else {
                                tracing::info!("successfully downloaded external file bytes");
                                tests_complete += 1;
                            }
                        }
                    }

                    test! {
                        "get guild channels",
                        channel::get_guild_channels(&client, GuildId::new(data.guild)),
                        |response| {
                            check!(response.channels.len(), 1);
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "create channel",
                        channel::create_channel(
                            &client,
                            CreateChannel::new(data.guild, "test".to_string(), Place::bottom(data.channel)),
                        ),
                        |response| {
                            tests_complete += 1;
                            test! {
                                "get channels compare new",
                                channel::get_guild_channels(&client, GuildId::new(data.guild)),
                                |response| {
                                    check!(response.channels.len(), 2);
                                    tests_complete += 1;
                                }
                            }
                            test! {
                                "delete channel",
                                channel::delete_channel(&client, DeleteChannel::new(data.guild, response.channel_id)),
                                |response| {
                                    tests_complete += 1;
                                    test! {
                                        "get channels compare delete",
                                        channel::get_guild_channels(&client, GuildId::new(data.guild)),
                                        |response| {
                                            check!(response.channels.len(), 1);
                                            tests_complete += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    test! {
                        "get guild information",
                        guild::get_guild(&client, GuildId::new(data.guild)),
                        |response| {
                            tests_complete += 1;
                        }
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
                            tests_complete += 1;
                            test! {
                                "compare new info",
                                guild::get_guild(&client, GuildId::new(data.guild)),
                                |response| {
                                    check!(response.guild_name, new_name);
                                    tests_complete += 1;
                                }
                            }
                        }
                    }

                    test! {
                        "create guild",
                        guild::create_guild(&client, CreateGuild::new("test".to_string())),
                        |response| {
                            tests_complete += 1;
                            test! {
                                "delete guild",
                                guild::delete_guild(&client, GuildId::new(response.guild_id)),
                                |response| {
                                    tests_complete += 1;
                                }
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
                            tests_complete += 1;
                        }
                    }

                    test! {
                        "set profile offline",
                        profile::profile_update(
                            &client,
                            ProfileUpdate::default().new_status(harmonytypes::UserStatus::Offline),
                        ),
                        |response| {
                            tests_complete += 1;
                            test! {
                                "compare profile status",
                                profile::get_user(&client, UserId::new(user_id)),
                                |response| {
                                    check!(response.user_status, i32::from(harmonytypes::UserStatus::Offline));
                                    tests_complete += 1;
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
                            tests_complete += 1;
                            test! {
                                "compare profile bot",
                                profile::get_user(&client, UserId::new(user_id)),
                                |response| {
                                    check!(response.is_bot, true);
                                    tests_complete += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    tests_complete
}

#[macro_export]
macro_rules! test {
    {
        $name:expr,
        $res:expr,
        |$val:ident| $sub:expr
    } => {
        info!("Testing {}...", $name);
        let ins = Instant::now();
        let span = info_span!($name);
        async {
            match $res.await {
                Ok($val) => {
                    info!("successful in {} ns", ins.elapsed().as_nanos());
                    info!("response: {:?}", $val);
                    $sub
                },
                Err(err) => error!("error occured: {}", err),
            }
        }.instrument(span).await
    };
    ($res:expr) => {
        test!($res, |_| {});
    };
}

#[macro_export]
macro_rules! check {
    ($res:expr, $res2:expr) => {
        if $res != $res2 {
            error!("check unsuccessful: {:?} != {:?}", $res, $res2);
        }
    };
}

use std::fmt;
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
        let level = event.metadata().level();

        if let Some(lvl) = level
            .eq(&Level::WARN)
            .then(|| "warning")
            .or_else(|| level.eq(&Level::ERROR).then(|| "error"))
        {
            write!(writer, "::{}::", lvl)?;

            // Write spans and fields of each span
            ctx.visit_spans(|span| {
                write!(writer, "{}:", span.name())?;
                Ok(())
            })?;

            ctx.field_format().format_fields(writer, event)?;

            writeln!(writer)
        } else {
            write!(writer, "")
        }
    }
}

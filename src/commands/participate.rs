use serenity::framework::standard::{macros::command, ArgError, Args, CommandResult};
use serenity::model::prelude::*;
use serenity::{prelude::*, utils::MessageBuilder};

use log::info;

use nvim_rs::{create::tokio as create, rpc::handler::Dummy as DummyHandler};

use tokio::process::Command;

use std::fs::File;
use vim_golf_bot::challenge::Challenge;

pub async fn create_nvim_instance(
) -> nvim_rs::neovim::Neovim<nvim_rs::compat::tokio::Compat<tokio::process::ChildStdin>> {
    const NVIMPATH: &str = "nvim";
    let handler = DummyHandler::new();

    let (nvim, _io_handle, _child) = create::new_child_cmd(
        Command::new(NVIMPATH)
            .args(&["-u", "NONE", "--embed", "--headless", "-Z", "--noplugin"])
            .env("NVIM_LOG_FILE", "nvimlog"),
        handler,
    )
    .await
    .unwrap();

    nvim
}

#[command]
#[description = r##"Participate to a challenge.
This command should be called with two arguments : a challenge ID and keys (as in map rhs)
Both of the ID and the keys can be escaped within backticks.
"##]
pub async fn participate(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let mut chal_id;
    if args.len() >= 2 {
        chal_id = args.single::<Challenge>();
    } else {
        chal_id = Challenge::last().ok_or(ArgError::from(String::from("No challenge to open.")));
    }

    let keys: &str = &args.single::<String>().unwrap();

    match chal_id {
        Ok(ref mut chall) => {
            let keys = keys.strip_prefix('`').unwrap_or(keys);
            let keys = keys.strip_suffix('`').unwrap_or(keys);

            let nvim = create_nvim_instance().await;
            let buf = nvim.create_buf(false, true).await?;
            let win = nvim.get_current_win().await?;

            win.set_buf(&buf).await?;
            let keys_parsed = nvim.replace_termcodes(keys, true, true, true).await?;

            buf.set_lines(0, -1, false, chall.input.content.to_vec()).await?;

            info!(
                "Feeding : {}",
                keys_parsed.escape_default().collect::<String>()
            );
            nvim.feedkeys(&keys_parsed, "ntx", true).await?;

            let err = nvim.get_vvar("errmsg").await.unwrap();

            let out_lines = buf.get_lines(0, -1, false).await?;
            if chall.output.content.eq(&out_lines) {
                let score = keys_parsed.len();
                chall.add_submission(msg.author.name.to_string(), keys.to_owned(), score);
                msg.reply(
                    ctx,
                    format!("Your submission is valid ! Your score is : {}", score),
                )
                .await?;

                let file = File::create(Challenge::filename(&chall.id))?;
                ron::ser::to_writer(file, &chall)?;
            } else {
                let mut builder = MessageBuilder::new();
                builder
                    .push_underline("Invalid answer")
                    .push(", your result is : ")
                    .push_line("```")
                    .push_line(out_lines.join("\n"))
                    .push_line("```");

                if let Some(err) = err.as_str() {
                    if !err.is_empty() {
                        builder
                            .push_line("")
                            .push_line("An error occurred when executing your input :")
                            .push_line("```")
                            .push_line(err)
                            .push_line("```");
                    }
                }

                msg.reply(ctx, builder.build()).await?;
            }
        }
        Err(inner) => {
            msg.reply(
                ctx,
                "Invalid command, usage : {prefix}participate [challenge_id] input",
            )
            .await?;
            info!("Failed to open challenge : {}", inner);
        }
    }

    Ok(())
}

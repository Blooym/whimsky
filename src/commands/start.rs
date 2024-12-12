use super::{ExecutableCommand, GlobalArguments};
use crate::bsky::{BlueskyHandler, PostData, PostEmbed};
use crate::database::Database;
use crate::fetcher::NikkiNewsFetcher;
use anyhow::Result;
use chrono::Duration;
use clap::Parser;
use reqwest::Url;
use std::primitive;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Start the bot and begin checking for news posts on an interval.
#[derive(Debug, Parser)]
pub struct StartCommand {
    /// The base URL of the service to communicate with.
    ///
    /// Note that that you must delete the file at `{data-path}/agentconfig.json` to change this after it has been initially set.
    #[clap(
        default_value = "https://bsky.social",
        long = "app-service",
        env = "WHIMSKY_APP_SERVICE"
    )]
    service: Url,

    /// The username or email of the application's account.
    #[clap(
        required = true,
        long = "app-identifier",
        env = "WHIMSKY_APP_IDENTIFIER"
    )]
    identifier: String,

    /// The app password to use for authentication.
    #[clap(required = true, long = "app-password", env = "WHIMSKY_APP_PASSWORD")]
    password: String,

    /// The interval of time in seconds between checking for news.
    #[clap(
        default_value_t = 300,
        long = "rerun-interval-seconds",
        env = "WHIMSKY_RERUN_INTERVAL_SECONDS"
    )]
    run_interval_seconds: u64,

    /// The number of hours in the past the bot should check for news that hasn't been posted.
    ///
    /// It is recommended to keep this to at least "1" as otherwise posts may get missed.
    #[clap(
        default_value_t = 3,
        long = "news-backdate-hours",
        env = "WHIMSKY_NEWS_BACKDATE_HOURS"
    )]
    news_backdate_hours: u16,

    /// Whether Bluesky posts should have comments disabled.
    #[clap(
        default_value_t = true,
        long = "disable-post-comments",
        env = "WHIMSKY_DISABLE_POST_COMMENTS"
    )]
    disable_post_comments: primitive::bool,

    /// The locale to use when fetching news posts.
    ///
    /// Existing options so far appear to be "en", "kr" and "ja".
    #[clap(
        default_value = "en",
        long = "news-locale",
        env = "WHIMSKY_NEWS_LOCALE",
        value_delimiter = ','
    )]
    news_locale: String,

    /// A comma-seperated list of languages in ISO-639-1 format to classify posts under.
    /// This should corrolate to the language of the posts the feed is linking to.
    #[clap(
        default_value = "en",
        long = "post-languages",
        env = "WHIMSKY_POST_LANGUAGES",
        value_delimiter = ','
    )]
    post_languages: Vec<String>,
}

impl ExecutableCommand for StartCommand {
    async fn run(self, global_args: GlobalArguments) -> Result<()> {
        let database = Database::new(&global_args.database_url).await?;
        let bsky_handler = BlueskyHandler::new(
            self.service,
            global_args.data_path,
            self.disable_post_comments,
        )
        .await?;
        bsky_handler.login(&self.identifier, &self.password).await?;

        let mut news_fetcher = NikkiNewsFetcher::new(
            self.news_locale,
            &database,
            Duration::hours(self.news_backdate_hours as i64),
        );
        loop {
            bsky_handler.sync_session().await.unwrap();
            info!(
                "Checking for unposted entries for news url {}",
                news_fetcher.get_news_url()
            );

            if let Ok(posts) = news_fetcher.fetch_unposted().await {
                for post in posts {
                    info!("Running for post '{}'", post.url);

                    let post_data = {
                        PostData {
                            created_at: post.publish_time,
                            text: format!("{} - {}", post.title, post.url),
                            languages: self.post_languages.clone(),
                            embed: Some(PostEmbed {
                                title: post.title,
                                description: post.r#abstract,
                                thumbnail_url: Some(post.cover),
                                uri: post.url.clone(),
                            }),
                        }
                    };
                    bsky_handler.post(post_data).await.unwrap();
                    database.add_posted_url(post.url.as_str()).await.unwrap();
                }
                if let Err(err) = database.remove_old_stored_posts().await {
                    warn!("Failed to run query to remove old stored posts {err}");
                }
            } else {
                error!(
                    "Failed to fetch news from {}: skipping for this iteration",
                    news_fetcher.get_news_url()
                );
            };
            info!(
                "Now waiting for {} seconds before re-running",
                self.run_interval_seconds
            );
            sleep(std::time::Duration::from_secs(self.run_interval_seconds)).await;
        }
    }
}

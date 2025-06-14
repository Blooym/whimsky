use anyhow::{Context, Result};
use bsky_sdk::{
    BskyAgent,
    agent::config::{Config, FileStore},
    api::{
        app::bsky::{
            embed::external::{ExternalData, MainData},
            feed::post::{self, RecordEmbedRefs},
        },
        types::{
            Collection, TryIntoUnknown, Union,
            string::{Datetime, Language, RecordKey},
        },
    },
    rich_text::RichText,
};
use chrono::{DateTime, Utc};
use image::{ImageFormat, ImageReader, imageops::FilterType};
use reqwest::Url;
use std::{io::Cursor, path::PathBuf, str::FromStr};
use tracing::{debug, info};

pub struct BlueskyHandler {
    pub agent: BskyAgent,
    pub data_path: PathBuf,
    pub disable_comments: bool,
}

#[derive(Debug)]
pub struct PostData {
    pub text: String,
    pub languages: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub embed: Option<PostEmbed>,
}

#[derive(Debug)]
pub struct PostEmbed {
    pub title: String,
    pub description: String,
    pub uri: Url,
    pub thumbnail_url: Option<Url>,
}

impl BlueskyHandler {
    fn make_default_config(service: &str) -> Config {
        Config {
            endpoint: service
                .to_string()
                .strip_suffix("/")
                .map_or(service.to_string(), |s| s.to_string()),
            ..Default::default()
        }
    }

    pub async fn new(
        service: Url,
        data_path_base: PathBuf,
        disable_comments: bool,
    ) -> Result<Self> {
        let data_path = data_path_base.join("agentconfig.json");

        // Try login with cached token.
        match Config::load(&FileStore::new(&data_path)).await {
            Ok(config) => {
                // We have a cached token, attempt to use it.
                match BskyAgent::builder().config(config).build().await {
                    Ok(agent) => {
                        let handler = Self {
                            agent,
                            data_path,
                            disable_comments,
                        };
                        handler.sync_session().await?;
                        Ok(handler)
                    }
                    Err(_) => Ok(Self {
                        // Using that session failed, make a new one.
                        agent: BskyAgent::builder()
                            .config(Self::make_default_config(service.as_str()))
                            .build()
                            .await?,
                        data_path,
                        disable_comments,
                    }),
                }
            }
            Err(_) => Ok(Self {
                // We don't cache a cached token, make a new session.
                agent: BskyAgent::builder()
                    .config(Self::make_default_config(service.as_str()))
                    .build()
                    .await?,
                data_path,
                disable_comments,
            }),
        }
    }

    pub async fn login(&self, identifier: &str, password: &str) -> Result<()> {
        self.agent.login(identifier, password).await?;
        self.sync_session().await?;
        Ok(())
    }

    pub async fn sync_session(&self) -> Result<()> {
        debug!("syncing agent session data");
        self.agent
            .to_config()
            .await
            .save(&FileStore::new(&self.data_path))
            .await
            .context("unable to sync bsky session")?;
        Ok(())
    }

    pub async fn post(&self, post: PostData) -> Result<()> {
        info!("Constructing post data for: '{}'", &post.text);
        let rt = RichText::new_with_detect_facets(&post.text).await?;
        let embed = match post.embed {
            Some(data) => Some(
                self.embed_external(
                    &data.title,
                    &data.description,
                    data.uri.as_ref(),
                    data.thumbnail_url,
                )
                .await?,
            ),
            None => None,
        };

        info!("Creating post record for: '{}'", &post.text);
        let record = self
            .agent
            .create_record(post::RecordData {
                created_at: Datetime::from_str(&post.created_at.fixed_offset().to_rfc3339())?,
                embed,
                entities: None,
                facets: rt.facets,
                labels: None,
                langs: Some(
                    post.languages
                        .iter()
                        .map(|f| Language::from_str(f).unwrap())
                        .collect(),
                ),
                reply: None,
                tags: None,
                text: post.text,
            })
            .await?;

        if self.disable_comments {
            info!(
                "Disabling post comments via threadgate for '{}'",
                record.uri
            );
            let rkey = record.uri.rsplit_once('/').map(|(_, rkey)| {
                RecordKey::from_str(rkey).expect("record key from post should always be vlaid")
            });

            self.agent
                .api
                .com
                .atproto
                .repo
                .create_record(
                    bsky_sdk::api::com::atproto::repo::create_record::InputData {
                        collection: bsky_sdk::api::app::bsky::feed::Threadgate::nsid(),
                        record: bsky_sdk::api::app::bsky::feed::threadgate::RecordData {
                            allow: Some(vec![]),
                            created_at: Datetime::now(),
                            hidden_replies: None,
                            post: record.uri.clone(),
                        }
                        .try_into_unknown()?,
                        repo: self
                            .agent
                            .get_session()
                            .await
                            .expect("not unauthenticated")
                            .data
                            .did
                            .into(),
                        rkey,
                        swap_commit: None,
                        validate: None,
                    }
                    .into(),
                )
                .await?;
        };

        Ok(())
    }

    async fn embed_external(
        &self,
        title: &str,
        description: &str,
        uri: &str,
        thumbnail_url: Option<Url>,
    ) -> Result<Union<RecordEmbedRefs>> {
        info!("Constructing external embed data for: '{uri}'");

        let thumb = if let Some(data) = thumbnail_url {
            debug!("Fetching and uploading image blob data for '{uri}'");
            let image_bytes = reqwest::get(data).await?.bytes().await?;
            let buf = (|| -> Result<Vec<u8>> {
                let mut buf: Vec<u8> = vec![];
                ImageReader::new(Cursor::new(&image_bytes))
                    .with_guessed_format()?
                    .decode()?
                    .resize(960, 540, FilterType::Nearest)
                    .write_to(&mut Cursor::new(&mut buf), ImageFormat::Jpeg)?;
                Ok(buf)
            })()
            .unwrap_or_else(|err| {
                debug!("Failed to convert image data: {err} - using original bytes");
                image_bytes.to_vec()
            });
            let output = self.agent.api.com.atproto.repo.upload_blob(buf).await?;
            Some(output.data.blob)
        } else {
            None
        };

        Ok(Union::Refs(RecordEmbedRefs::AppBskyEmbedExternalMain(
            Box::new(
                MainData {
                    external: ExternalData {
                        description: description.into(),
                        title: title.into(),
                        uri: uri.into(),
                        thumb,
                    }
                    .into(),
                }
                .into(),
            ),
        )))
    }
}

use crate::database::Database;
use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use reqwest::Url;
use serde::Deserialize;
use tracing::debug;

pub struct NikkiNewsFetcher<'a> {
    filter_date: chrono::DateTime<Utc>,
    database: &'a Database,
    backdate_duration: Duration,
    news_url: Url,
    locale: String,
}

#[derive(Debug, Deserialize)]
pub struct NikkiNewsResponse {
    pub data: NikkiNewsData,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NikkiNewsData {
    pub total: usize,
    pub data: Vec<NikkiNewsDataInner>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct NikkiNewsDataInner {
    pub id: usize,
    pub title: String,
    pub section: usize,
    // pub info: Option<String>, // Not actually sure the inner type of this yet
    pub publish_time: DateTime<Utc>,
    pub cover: Url,
    pub r#abstract: String,
}

pub struct NikkiNewsPost {
    pub url: Url,
    pub title: String,
    pub publish_time: DateTime<Utc>,
    pub cover: Url,
    pub r#abstract: String,
}

impl<'a> NikkiNewsFetcher<'a> {
    fn make_news_url(locale: &str, limit: usize) -> Url {
        Url::parse(&format!(
            "https://infinitynikki.infoldgames.com/api/news?offset=0&limit={}&locale={}",
            limit, locale
        ))
        .unwrap()
    }

    pub fn new(locale: String, database: &'a Database, feed_backdate: Duration) -> Self {
        let news_url = Self::make_news_url(&locale, 20);
        let filter_date = Utc::now() - feed_backdate;
        debug!(
            "Initializing news fetcher for {news_url} with starting filter date of {filter_date}"
        );

        Self {
            database,
            news_url,
            filter_date,
            locale,
            backdate_duration: feed_backdate,
        }
    }

    pub fn get_news_url(&self) -> &Url {
        &self.news_url
    }

    pub async fn fetch_unposted(&mut self) -> Result<Vec<NikkiNewsPost>> {
        let mut content = reqwest::get(self.news_url.as_str())
            .await?
            .json::<NikkiNewsResponse>()
            .await?;
        content.data.data.dedup_by_key(|k| k.id);
        content.data.data.sort_by_key(|k| k.id);
        content.data.data.reverse();

        let mut posts = vec![];
        for item in content.data.data {
            // Only count posts that are after the filter date.
            if item.publish_time <= self.filter_date {
                continue;
            }

            let link = Url::parse(&format!(
                "https://infinitynikki.infoldgames.com/{}/news/{}",
                self.locale, item.id
            ))?;
            if self.database.has_posted_url(link.as_str()).await? {
                continue;
            }

            posts.push(NikkiNewsPost {
                r#abstract: item.r#abstract.trim().to_string(),
                cover: item.cover,
                publish_time: item.publish_time,
                title: item.title.trim().to_string(),
                url: link,
            });
        }
        self.filter_date = Utc::now() - self.backdate_duration;
        Ok(posts)
    }
}

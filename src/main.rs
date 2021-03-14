// #![feature(async_closure)]
#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(unused_results)]
#[macro_use]
extern crate clap;
use clap::App;
use rayon::prelude::*;
use roux::Subreddit;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;
use indicatif::ProgressBar;
use priority_queue::PriorityQueue;
use rand::Rng;
use roux::Reddit;
use rusqlite::Connection;
use rusqlite::{params, NO_PARAMS};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::File;

use std::io::{prelude::*, BufReader};
use tokio;
extern crate edit_distance;
use edit_distance::edit_distance;

const MAX_PAGES: u32 = 50;
const MAX_RESULTS_PER_PAGE: u32 = 1000;
const GRAPH_PATH: &str = "web/graph.json";

fn create_db(path: &str) -> rusqlite::Result<rusqlite::Connection> {
    let db = Connection::open(path)?;

    db.execute(
        "create table if not exists subreddits (
             id          text primary key,
             name        text not null unique,
             subscribers UNSIGNED BIG INT
         )",
        NO_PARAMS,
    )?;

    db.execute(
        "create table if not exists posts (
             id           text primary key,
             author       text  not null,
             created_utc  float not null,
             permalink    text  not null,
             subreddit    text  not null,
             subreddit_id text  not null,
             title        text  not null
         )",
        NO_PARAMS,
    )?;

    Ok(db)
}

async fn add_subreddits_to_db(path: &str, db: &rusqlite::Connection) {
    info!("Creating and populating / updating database");

    let wc = {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        reader.lines().count() as u64
    };

    let file = File::open(path).unwrap();
    let reader = BufReader::new(file);
    // let bar = ProgressBar::new(wc);

    for line in reader.lines() {
        // bar.inc(1);
        let name = line.unwrap();
        info!("Found r/{} in file.", name);

        match Subreddit::new(&name).about().await {
            Ok(about) => match about.display_name {
                Some(proper_name) => {
                    db.execute(
                        "INSERT OR IGNORE INTO subreddits (id, name, subscribers) values (?1, ?2, ?3)",
                        &[&about.id.unwrap(), &proper_name, &about.subscribers.unwrap_or(0).to_string()],
                    )
                    .unwrap();
                }
                None => {
                    error!(
                        "Subreddit with name '{}' from file has no name in about()",
                        name
                    );
                }
            },
            Err(e) => {
                error!("Could not retrieve about for r/{}: {}", name, e);
            }
        }
    }
    // bar.finish();
}

fn build_sample_graph() -> ForceGraph {
    ForceGraph {
        nodes: vec![
            Node {
                id: "r/rust".to_string(),
                group: 1,
                scale: 1.,
            },
            Node {
                id: "r/programming".to_string(),
                group: 1,
                scale: 1.2,
            },
            Node {
                id: "r/cpp".to_string(),
                group: 1,
                scale: 0.8,
            },
        ],
        links: vec![
            Link {
                source: "r/rust".to_string(),
                target: "r/programming".to_string(),
                weight: 5.0,
            },
            Link {
                source: "r/programming".to_string(),
                target: "r/cpp".to_string(),
                weight: 3.0,
            },
        ],
    }
}

fn build_graph(db: &rusqlite::Connection) -> rusqlite::Result<ForceGraph> {
    info!("Building graph");

    let mut rng = rand::thread_rng();
    let mut graph = ForceGraph {
        nodes: Vec::new(),
        links: Vec::new(),
    };

    db.prepare("SELECT id, name, subscribers FROM subreddits ORDER BY subscribers DESC LIMIT 200")?
        .query_map(rusqlite::NO_PARAMS, |row| {
            Ok(SubredditDb {
                id: row.get(0)?,
                name: row.get(1)?,
                subscribers: row.get(2).unwrap_or_default(),
            })
        })?
        .filter(|iter| iter.is_ok())
        .map(|iter| iter.unwrap())
        .for_each(|sr| {
            if rng.gen_range(0..1) == 0 {
                graph.nodes.push(Node {
                    id: sr.name,
                    group: rng.gen_range(0..10),
                    scale: 0.5 + (sr.subscribers as f64).log2() / 32.0,
                });
            }
        });

    let similarities = calculate_similarities(&db)?;

    for i in 0..graph.nodes.len() {
        let x = &graph.nodes[i];
        let mut pq = PriorityQueue::new();

        for j in i + 1..graph.nodes.len() {
            let y = &graph.nodes[j];

            let weight = similarities
                .get(&(x.id.clone(), y.id.clone()))
                .unwrap_or(&0.0)
                .clone();
            if weight > 0.1 {
                pq.push(&y.id, weight.round() as u32);
            }
        }

        for _ in 0..10 {
            match pq.peek() {
                Some((y_id, weight)) => {
                    // add overlapping edges
                    for _ in 0..std::cmp::max(1, (*weight) / 10) {
                        graph.links.push(Link {
                            source: x.id.clone(),
                            target: y_id.to_string(),
                            weight: f64::log10(*weight as f64) / 5.0,
                        });
                    }
                }
                None => {
                    break;
                }
            }
        }
    }

    /*
    for i in 0..graph.nodes.len() {
        let x = &graph.nodes[i];

        let js: Vec<(usize, f64)> = (i + 1..graph.nodes.len())
            .into_par_iter()
            .map(|j| {
                let y = &graph.nodes[j];
                // let dist = edit_distance(&x.id, &y.id) as f64;
                // let weight = 1.0 - (dist / (x.id.len() + y.id.len()) as f64);
                let weight = similarities
                    .get(&(x.id.clone(), y.id.clone()))
                    .unwrap_or(&0.0)
                    .clone();
                (j, weight) //f64::log2(weight)) // / x.scale)
            })
            .filter(|(j, weight)| weight >= &6.0)
            .collect();

        for (j, weight) in js {
            let link = Link {
                source: x.id.clone(),
                target: graph.nodes[j].id.clone(),
                weight: f64::log10(weight),
            };
            graph.links.push(link);
        }
    }*/

    let n = graph.nodes.len() as f64;
    let m = graph.links.len() as f64;
    let density = m / (n * (n - 1.) / 2.);
    info!(
        "Generated a graph with {} nodes, {} edges ({:.2}% density)",
        n,
        m,
        density * 100.0
    );

    Ok(graph)
}

fn build_json(graph: &ForceGraph) -> serde_json::Result<String> {
    serde_json::to_string(graph)
}

#[derive(Serialize, Deserialize)]
struct Node {
    id: String,
    group: u64,
    scale: f64,
}

#[derive(Serialize, Deserialize)]
struct Link {
    source: String,
    target: String,
    weight: f64,
}

#[derive(Serialize, Deserialize)]
struct ForceGraph {
    nodes: Vec<Node>,
    links: Vec<Link>,
}

struct SubredditDb {
    id: String,
    name: String,
    subscribers: u32,
}

async fn update_subreddit_size(
    db: &rusqlite::Connection,
    just_fill_missing: bool,
) -> rusqlite::Result<()> {
    let mut stmt = db.prepare("SELECT id, name, subscribers FROM subreddits")?;
    let subreddit_iter = stmt.query_map(rusqlite::NO_PARAMS, |row| {
        Ok(SubredditDb {
            id: row.get(0)?,
            name: row.get(1)?,
            subscribers: row.get(2).unwrap_or_default(),
        })
    })?;

    for iter in subreddit_iter {
        let sr = iter?;
        let name = &sr.name;
        let subreddit = Subreddit::new(name);

        if sr.subscribers > 0 && just_fill_missing {
            info!("Skipping r/{}", name);
            continue;
        }

        match subreddit.about().await {
            Ok(about) => match about.subscribers {
                Some(x) => {
                    info!("Retrieved new sub count for r/{} = {}", name, x);
                    db.execute(
                        "UPDATE subreddits SET subscribers = ?1 WHERE name = ?2",
                        params![x, name],
                    )
                    .expect("could execute update SQL");
                }
                None => {
                    error!("Retrieved metadata for r/{} but there's no sub count", name)
                }
            },
            Err(e) => {
                error!("Could not retrieve metadata for r/{}", name);
            }
        }
    }

    Ok(())
}

async fn add_recent_posts_to_db(db: &rusqlite::Connection, max_pages: u32) -> rusqlite::Result<()> {
    let mut stmt = db.prepare(
        "SELECT id, name, subscribers FROM subreddits ORDER BY subscribers DESC LIMIT 200",
    )?;
    let subreddits: Vec<SubredditDb> = stmt
        .query_map(rusqlite::NO_PARAMS, |row| {
            Ok(SubredditDb {
                id: row.get(0)?,
                name: row.get(1)?,
                subscribers: row.get(2).unwrap_or_default(),
            })
        })?
        .filter(|iter| iter.is_ok())
        .map(|iter| iter.unwrap())
        .collect();

    for sr in subreddits {
        let subreddit = roux::Subreddit::new(&sr.name);
        let mut after: Option<String> = None;

        for page in 0..max_pages {
            info!(
                "Retrieving page {}/{} of r/{}",
                page, max_pages, subreddit.name
            );

            let mut options = roux::util::FeedOption::new();
            options.after = after.clone();

            let latest = subreddit.latest(MAX_RESULTS_PER_PAGE, Some(options));
            match latest.await {
                Ok(resp) => {
                    after = resp.data.after;
                    let posts = resp.data.children;

                    for post in posts {
                        info!(
                            "Found post on r/{}. Adding to db: {}",
                            &subreddit.name, &post.data.title
                        );
                        if db.execute("INSERT OR IGNORE INTO posts (id, author, created_utc, permalink, subreddit, subreddit_id, title)
                                    values (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                                    &[post.data.id, post.data.author, post.data.created_utc.to_string(), post.data.permalink, post.data.subreddit, post.data.subreddit_id, post.data.title],
                                    ).is_err() {
                                        error!("Could not add post to db (r/{})", subreddit.name);
                                    }
                    }
                }
                Err(e) => error!("Could not retrieve data for r/{}", subreddit.name),
            }

            if after.is_none() {
                break;
            }
        }
    }

    Ok(())
}

fn calculate_similarities(
    db: &rusqlite::Connection,
) -> rusqlite::Result<HashMap<(String, String), f64>> {
    let mut stmt = db.prepare(
        "SELECT p1.subreddit,
                p2.subreddit,
                count(distinct(p1.author))
         FROM posts AS p1
         JOIN posts AS p2 ON p1.author=p2.author
         WHERE p1.subreddit != p2.subreddit AND
               p1.author != '[deleted]' AND
               p1.author != 'AutoModerator'
         GROUP BY p1.subreddit, p2.subreddit",
    )?;

    let mut similarities = HashMap::new();
    let mut rows = stmt.query(NO_PARAMS)?;
    while let Some(row) = rows.next().expect("") {
        let s1: String = row.get(0).unwrap();
        let s2: String = row.get(1).unwrap();
        let cnt: u32 = row.get(2).unwrap();

        let sim = cnt as f64;

        info!("similarity(r/{}, r/{}) == {}", s1, s2, sim);

        similarities.insert((s1.clone(), s2.clone()), sim);
        similarities.insert((s2.clone(), s1.clone()), sim);
    }

    Ok(similarities)
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let yaml = load_yaml!("cli.yml");
    let matches = App::from_yaml(yaml).get_matches();

    let db_file = matches.value_of("db").expect("No database provided (--db $PATH_TO_DB)");
    let db = create_db(db_file).expect("Could not connect to database");

    if let Some(matches) = matches.subcommand_matches("init") {
        let subreddit_file = matches.value_of("subreddit_file").unwrap();
        add_subreddits_to_db(subreddit_file, &db).await;

        // This should never be needed anymore
        // update_subreddit_size(&db, false)
        //     .await
        //     .expect("could not update subreddit size");
    }


    if let Some(matches) = matches.subcommand_matches("scrape") {
        let max_pages: u32 = match matches.value_of("max_pages") {
            Some(val) => val.parse().unwrap_or(MAX_PAGES),
            None => MAX_PAGES,
        };

        add_recent_posts_to_db(&db, max_pages)
            .await
            .expect("could not add recent posts to db");
    }

    if let Some(matches) = matches.subcommand_matches("graph") {
        let graph = build_graph(&db).expect("Could not build graph.");
        let js = serde_json::to_string(&graph).expect("Could not serialize graph to JSON.");
        fs::write(GRAPH_PATH, js).expect(&format!("Could not write JSON to '{}'", GRAPH_PATH));
    }
}

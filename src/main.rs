#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(unused_results)]
use rayon::prelude::*;
use roux::Subreddit;
extern crate pretty_env_logger;
#[macro_use]
extern crate log;
use rand::Rng;
use rusqlite::Connection;
use rusqlite::NO_PARAMS;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{prelude::*, BufReader};
use tokio;
extern crate edit_distance;
use edit_distance::edit_distance;

fn create_db() -> rusqlite::Result<rusqlite::Connection> {
    let db = Connection::open("mappit.db")?;

    db.execute(
        "create table if not exists subreddits (
             id integer primary key,
             name text not null unique,
             subscribers UNSIGNED BIG INT
         )",
        NO_PARAMS,
    )?;
    // db.execute(
    //     "create table if not exists cats (
    //          id integer primary key,
    //          name text not null,
    //          color_id integer not null references cat_colors(id)
    //      )",
    //     NO_PARAMS,
    // )?;

    Ok(db)
}

fn add_subreddits_to_db(db: &rusqlite::Connection) {
    info!("Creating and populating / updating database");
    let file = File::open("data/subreddits.sorted").unwrap();
    let reader = BufReader::new(file);

    for line in reader.lines() {
        let subreddit = line.unwrap();
        db.execute(
            "INSERT OR IGNORE INTO subreddits (name) values (?1)",
            &[&subreddit.to_string()],
        )
        .unwrap();
    }
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
                value: 5.0,
            },
            Link {
                source: "r/programming".to_string(),
                target: "r/cpp".to_string(),
                value: 3.0,
            },
        ],
    }
}

fn build_graph(db: &rusqlite::Connection) -> rusqlite::Result<ForceGraph> {
    let mut stmt = db.prepare("SELECT id, name, subscribers FROM subreddits")?;
    let subreddit_iter = stmt.query_map(rusqlite::NO_PARAMS, |row| {
        Ok(SubredditDb {
            id: row.get(0)?,
            name: row.get(1)?,
            subscribers: row.get(2).unwrap_or_default(),
        })
    })?;

    let mut graph = ForceGraph {
        nodes: Vec::new(),
        links: Vec::new(),
    };

    let mut rng = rand::thread_rng();
    for sr in subreddit_iter {
        let node = Node {
            id: sr?.name,
            group: rng.gen_range(0..10),
            scale: 1.0,
        };

        if rng.gen_range(0..10) == 0 {
            graph.nodes.push(node);
        }
    }

    for i in 0..graph.nodes.len() {
        let x = &graph.nodes[i];

        let js: Vec<(usize, f64)> = (i + 1..graph.nodes.len())
            .into_par_iter()
            .map(|j| {
                let y = &graph.nodes[j];
                let dist = edit_distance(&x.id, &y.id) as f64;

                let weight = 1.0 - (dist / (x.id.len() + y.id.len()) as f64);
                (j, weight)
            })
            .filter(|(j, weight)| weight > &0.75)
            .collect();

        for (j, weight) in js {
            let link = Link {
                source: x.id.clone(),
                target: graph.nodes[j].id.clone(),
                value: weight,
            };
            graph.links.push(link);
        }
    }

    let n = graph.nodes.len() as f64;
    let m = graph.links.len() as f64;
    let density = m / (n * (n + 1.) / 2.);
    info!(
        "Generated a graph with {} nodes, {} edges ({:.3}% density)",
        n, m, density
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
    value: f64,
}

#[derive(Serialize, Deserialize)]
struct ForceGraph {
    nodes: Vec<Node>,
    links: Vec<Link>,
}

struct SubredditDb {
    id: u32,
    name: String,
    subscribers: u32,
}

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let db = create_db().expect("Could not create db connection");
    // add_subreddits_to_db(&db);

    // Get hot posts with limit = 25.
    let subreddit = Subreddit::new("rust");
    let hot = subreddit.hot(25, None).await;
    let after = hot.unwrap();

    let graph = build_graph(&db).expect("could not build graph");

    let js = serde_json::to_string(&graph).expect("could not serialize graph to json");
    fs::write("web/subreddit_graph.json", js).expect("could not write json to file");
}

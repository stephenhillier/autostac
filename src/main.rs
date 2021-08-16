#[macro_use] extern crate rocket;
use rocket::{Request, Response};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::http::Header;
use std::collections::HashMap;
use std::u8;
use serde::Deserialize;
use structopt::StructOpt;
use structopt_toml::StructOptToml;
mod handlers;
mod transform;
mod catalog;
mod stac;


#[derive(Debug, Deserialize, StructOpt, StructOptToml)]
#[serde(default)]
struct Opt {
    /// Directory to catalog.
    ///
    /// Subdirectories (one level deep) will be scanned to create collections.
    /// Imagery in subdirectories will be catalogued.
    /// 
    /// e.g. the subdirectories:
    ///
    ///     ./data/imagery
    ///     ./data/landuse
    ///
    /// will create two collections "imagery" and "landuse".  These collections will be
    /// populated by the files within their respective directories.
    #[structopt(default_value = "./data", long, short = "d", env = "AUTOSTAC_CATALOG_DIR")]
    dir: String,

    /// Autostac will catalog from S3.
    /// Warning: uses AWS_S3_ENDPOINT, AWS_S3_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY.
    /// Ensure these values are not set to values you don't want to use.
    ///
    /// this redundant option is in place to prevent accidently
    /// starting the application with AWS_ env vars set.
    #[structopt(long, requires="s3-host")]
    s3: bool,

    /// S3 host to use.
    #[structopt(long, env = "AWS_S3_ENDPOINT", requires="s3-bucket")]
    s3_host: Option<String>,

    /// S3 bucket to use as the root of the catalog.
    ///
    /// Collections will be built based on prefixes.
    #[structopt(long, env = "S3_BUCKET")]
    s3_bucket: Option<String>,

    /// S3 access key
    #[structopt(long, env = "AWS_ACCESS_KEY_ID")]
    s3_access_key: Option<String>,

    /// S3 secret key
    #[structopt(long, env = "AWS_SECRET_ACCESS_KEY")]
    s3_secret_key: Option<String>,

    #[structopt(long, env="AWS_REGION")]
    s3_region: Option<String>,

    /// ID of the service (used for the STAC landing page)
    #[structopt(default_value = "autostac", long, env = "AUTOSTAC_SERVICE_ID")]
    id: String,

    /// Title of the service
    #[structopt(default_value = "Autostac Demo", long, env = "AUTOSTAC_SERVICE_TITLE")]
    title: String,

    /// Description of the service
    #[structopt(default_value = "An automatic STAC API from a directory or S3 bucket", long, env = "AUTOSTAC_SERVICE_DESCRIPTION")]
    description: String,

    /// The base url that each collection will be advertised at.
    // this needs to be rethought to ensure the URL is in sync with the address
    // the service is listening at.
    #[structopt(default_value = "./data", long, env = "AUTOSTAC_BASE_URL")]
    base_url: String
}

pub struct CORS;

// https://github.com/SergioBenitez/Rocket/issues/25#issuecomment-838566038
#[rocket::async_trait]
impl Fairing for CORS {
    fn info(&self) -> Info {
        Info {
            name: "CORS headers",
            kind: Kind::Response,
        }
    }

    async fn on_response<'r>(&self, _request: &'r Request<'_>, response: &mut Response<'r>) {
        response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
        response.set_header(Header::new(
            "Access-Control-Allow-Methods",
            "GET, POST, OPTIONS",
        ));
        response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
    }
}

#[rocket::main]
async fn main() {

    let opt = Opt::from_args();
    let collections: HashMap<String, catalog::ImageryCollection>;

    // if s3_host was supplied, create collections from S3.
    if opt.s3_host.is_some() && opt.s3 {
        collections = catalog::collections_from_s3(
            &opt.s3_host.unwrap(),
            &opt.s3_bucket.unwrap(),
            &opt.s3_access_key.unwrap(),  // this shouldn't be required. todo: make it an Option.
            &opt.s3_secret_key.unwrap()   // ^
        ).await;
    } else {
        collections = catalog::collections_from_subdirs(&opt.dir);
    }

    // initialize a service catalog with some info about our service.
    // todo: these should be cli flags or read from a config file.
    let svc = catalog::Service {
        id: String::from("autostac"),
        title: String::from("Autostac Demo"),
        description: String::from("Demo for the autostac remote sensing raster data service"),
        base_url: url::Url::parse("http://localhost:8000").unwrap(),
        collections
    };

    // start application
    let _app = rocket::build()
        .attach(CORS)
        .manage(svc)
        // STAC conforming API.
        // routes are slowly being moved here.
        .mount(
            "/",
            routes![
            handlers::get_collection_item,
            handlers::get_collection,    
            handlers::get_tiles,
            handlers::search_all_preflight,
            handlers::search_all_collections,
            handlers::landing
            ]
        ).launch().await;
}

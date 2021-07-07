#[macro_use] extern crate rocket;
use std::collections::HashMap;
use std::convert::TryInto;
use std::f64;
use std::u32;
use std::u8;
use geo_types::Geometry;
use catalog::AsFeatureCollection;
use serde_json::{to_string};
use rocket::{State, response::content::Json};
use wkt::Wkt;
mod transform;
mod catalog;
mod stac;

/// returns a tile from a collection item covering the tile defined by its x/y/z address.
fn _tile(collection_id: String, z: u8, x:u32, y:u32, coverage: &State<catalog::Service>) -> String {
    let bounds: Geometry<f64> = transform::to_bounds(x, y, z).try_into().unwrap();
    let collection = coverage.collections.get(&collection_id).unwrap();
    
    // currently this just returns files that could provide coverage for the tile.
    // work in progress...
    let files_for_tile = collection.intersects(&bounds);

    // stand-in for an actual tile
    format!("{} {} {} :\n {:?} :\n {:?}", z, x, y, bounds, files_for_tile)
}

/// returns a GeoJSON FeatureCollection representing available imagery that intersects
/// with the polygon (in WKT format) provided by the `?intersects` query.
/// example:  /collections/imagery?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))
#[get("/collections/<collection_id>?<intersects>")]
fn collection_items_intersecting_polygon(collection_id: String, intersects: &str, coverage: &State<catalog::Service>) -> Json<String> {
    let wkt_geom = Wkt::from_str(intersects).ok().unwrap();
    let bounds: Geometry<f64> = wkt_geom.try_into().unwrap();
    let collection = coverage.collections.get(&collection_id).unwrap();

    let imagery = collection.intersects(&bounds).as_feature_collection();
    Json(to_string(&imagery).unwrap())
}

/// STAC API Item endpoint
/// returns a GeoJSON Feature representing the item.
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/item-spec/README.md
#[get("/collections/<collection_id>/<item_id>")]
fn get_collection_item(collection_id: String, item_id: String, coverage: &State<catalog::Service>) -> Json<String> {
    let collection = coverage.collections.get(&collection_id).unwrap();
    let item = collection.get_item(item_id).unwrap();
    Json(to_string(&item.to_stac_feature()).unwrap())
}

/// STAC API collections endpoint
/// Returns a STAC Collection JSON representation of the collection with ID `collection_id`
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/collection-spec/README.md
#[get("/collections/<collection_id>")]
fn get_collection(collection_id: String, coverage: &State<catalog::Service>) -> Json<String> {
    let collection = &coverage.collections.get(&collection_id)
        .unwrap().stac_collection(&coverage.base_url);
    Json(to_string(collection).unwrap())
}

/// STAC API landing page
/// based on https://github.com/radiantearth/stac-api-spec/blob/master/overview.md#example-landing-page
#[get("/")]
fn landing(coverage: &State<catalog::Service>) -> Json<String> {
    Json(to_string(&coverage.stac_landing()).unwrap())
}

#[launch]
fn rocket() -> _ {

    // for now, start with searching in the ./data dir.
    // subdirectories (one level deep) will be scanned to create collections.
    // imagery in subdirectories will be catalogued.
    // e.g. the subdirectories:
    //     ./data/imagery
    //     ./data/landuse
    // will create two collections "imagery" and "landuse".  These collections will be
    // populated by the files within their respective directories.
    let dir: &str = "./data";

    let collections: HashMap<String, catalog::ImageryCollection> = catalog::collections_from_subdirs("./data");

    // initialize a service catalog with some info about our service.
    // todo: these should be cli flags or read from a config file.
    let svc = catalog::Service {
        id: String::from("rs2"),
        title: String::from("RS2 Demo"),
        description: String::from("Demo for the rs2 remote sensing raster data service"),
        base_url: url::Url::parse("http://localhost:8000").unwrap(),
        collections
    };

    // start application
    rocket::build()
        .manage(svc)
        // STAC conforming API.
        // routes are slowly being moved here.
        .mount(
            "/",
            routes![
            collection_items_intersecting_polygon,
            get_collection_item,
            get_collection,    
            landing
            ]
        )
}

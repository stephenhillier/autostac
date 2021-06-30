#[macro_use] extern crate rocket;
use std::f64;
use std::u32;
use std::u8;
use geo::polygon;
use geojson::{FeatureCollection};
use geo_types::Polygon;
use raster::AsFeatureCollection;
use serde_json::{to_string};
use serde::{Serialize};
use rocket::{State, response::content::Json};
use tile_grid::Grid;

mod transform;
mod raster;


/// get_tile_bounds returns the tile boundaries (web mercator EPSG:3857)
/// for a tile from a URL with /z/x/y format.
fn get_tile_bounds(z: u8, x:u32, y:u32) -> Polygon<f64> {
    let grid = Grid::web_mercator();
    let extent = grid.tile_extent_xyz(x, y, z);
    println!("z:{}, x:{}, y:{}, extent: {:?}", z, x, y, extent);
    polygon!(
        (x: extent.minx, y: extent.miny),
        (x: extent.maxx, y: extent.miny),
        (x: extent.maxx, y: extent.maxy),
        (x: extent.minx, y: extent.maxy),
    )
}

#[get("/tile/<z>/<x>/<y>")]
fn tile(z: u8, x:u32, y:u32, coverage: &State<raster::Service>) -> String {
    let bounds = get_tile_bounds(z, x, y);
    let files_for_tile = &coverage.inner().imagery.intersects(&bounds);

    // stand-in for an actual tile
    format!("{} {} {} :\n {:?} :\n {:?}", z, x, y, bounds, files_for_tile)
}

/// CollectionsResponse represents the response schema
/// for a request to the collections endpoint.
/// it contains FeatureCollections for available data categories
#[derive(Debug, Serialize)]
struct CollectionsResponse {
    imagery: FeatureCollection,
    rasters: FeatureCollection
}

/// returns a set of GeoJSON FeatureCollections representing
/// data collections grouped into categories.
/// So far only imagery (satellite imagery etc) is supported.
/// TODO:  refactor collections into methods on their respective services.
#[get("/collections")]
fn collections(coverage: &State<raster::Service>) -> Json<String> {
    let imagery = coverage.imagery.all().as_feature_collection();
    let rasters = coverage.rasters.all().as_feature_collection();

    let collection = CollectionsResponse {
        imagery,
        rasters
    };
    Json(to_string(&collection).unwrap())
}

#[launch]
fn rocket() -> _ {
    // initialize raster coverage
    let svc = raster::Service {
        imagery: raster::ImageryRepository::new(),
        rasters: raster::RasterRepository::new()
    };

    // start application
    rocket::build()
        .manage(svc)
        .mount("/", routes![tile, collections])
}

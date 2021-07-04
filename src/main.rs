#[macro_use] extern crate rocket;
use std::convert::TryInto;
use std::f64;
use std::u32;
use std::u8;
use geojson::{FeatureCollection};
use geo_types::Polygon;
use raster::AsFeatureCollection;
use serde_json::{to_string};
use serde::{Serialize};
use rocket::{State, response::content::Json};
use wkt::Wkt;
mod transform;
mod raster;
mod stac;


#[get("/tiles/<z>/<x>/<y>")]
fn tile(z: u8, x:u32, y:u32, coverage: &State<raster::Service>) -> String {
    let bounds = transform::web_mercator_tile_bounds(z, x, y);
    let files_for_tile = &coverage.inner().imagery.intersects(&bounds);

    // stand-in for an actual tile
    format!("{} {} {} :\n {:?} :\n {:?}", z, x, y, bounds, files_for_tile)
}

/// returns a GeoJSON FeatureCollection representing available imagery that intersects
/// with the polygon (in WKT format) provided by the `?intersects` query.
/// example:  /api/v1/collections/imagery?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))
#[get("/collections/imagery?<intersects>")]
fn imagery_collection_intersecting_polygon(intersects: &str, coverage: &State<raster::Service>) -> Json<String> {
    let wkt_poly = Wkt::from_str(intersects).ok().unwrap();
    let bounds: Polygon<f64> = wkt_poly.try_into().unwrap();
    let imagery = coverage.imagery.intersects(&bounds).as_feature_collection();
    Json(to_string(&imagery).unwrap())
}

/// returns a GeoJSON FeatureCollection representing available raster data that intersects
/// with the polygon (in WKT format) provided by the `?intersects` query.
/// example:  /api/v1/collections/rasters?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))
#[get("/collections/rasters?<intersects>")]
fn rasters_collection_intersecting_polygon(intersects: &str, coverage: &State<raster::Service>) -> Json<String> {
    let wkt_poly = Wkt::from_str(intersects).ok().unwrap();
    let bounds: Polygon<f64> = wkt_poly.try_into().unwrap();
    let rasters = coverage.rasters.intersects(&bounds).as_feature_collection();
    Json(to_string(&rasters).unwrap())
}

/// returns a GeoJSON FeatureCollection representing available imagery
#[get("/collections/imagery")]
fn imagery_collection(coverage: &State<raster::Service>) -> Json<String> {
    let imagery = coverage.imagery.all().as_feature_collection();
    Json(to_string(&imagery).unwrap())
}

/// returns a GeoJSON FeatureCollection representing available imagery
#[get("/collections/rasters")]
fn raster_collection(coverage: &State<raster::Service>) -> Json<String> {
    let rasters = coverage.rasters.all().as_feature_collection();
    Json(to_string(&rasters).unwrap())
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

/// STAC API landing page
/// based on https://github.com/radiantearth/stac-api-spec/blob/master/overview.md#example-landing-page
#[get("/")]
fn landing() -> Json<String> {
    // use hardcoded defaults for now.
    // in the future, allow specifying id, title, description.
    let stac_landing = stac::LandingPage::new(
        String::from("rs2"),
        String::from("RS2 Demo"),
        String::from("Demo for the rs2 remote sensing raster data service"),
        String::from("https://example.org/")
    );
    Json(to_string(&stac_landing).unwrap())
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
        // these API routes do not conform to STAC.
        // routes are being converted and moved.
        .mount("/api/v1", routes![
            tile,
            rasters_collection_intersecting_polygon,
            imagery_collection_intersecting_polygon,
            imagery_collection,
            raster_collection,
            collections,
        ])
        // STAC conforming API.
        // routes are slowly being moved here.
        .mount(
            "/",
            routes![
                landing
            ]
        )
}

use std::convert::TryInto;
use std::f64;
use std::u32;
use std::u8;
use geo_types::Geometry;
use catalog::AsFeatureCollection;
use serde_json::{to_string};
use rocket::{State, response::content::Json};
use rocket::response::status::BadRequest;
use wkt::Wkt;
use crate::transform;
use crate::catalog;

/// returns a GeoJSON FeatureCollection representing available imagery that intersects
/// with the polygon (in WKT format) provided by the `?intersects` query.
/// example:  /collections/imagery?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))
#[get("/collections/<collection_id>?<intersects>")]
pub fn collection_items_intersecting_polygon(
  collection_id: String,
  intersects: &str,
  coverage: &State<catalog::Service>
) -> Result<Option<Json<String>>, BadRequest<String>> {

  // convert the intersects query into a Geometry.
  // WKT format is expected.
  // If any errors occur, respond to the request with a 400 error.
  let wkt_geom = match Wkt::from_str(intersects) {
      Ok(w) => w,
      Err(_) => return Err(BadRequest(
          Some("Invalid WKT in `intersects` query param. Example of a valid query: \
              ?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))".into()))),
  };

  let bounds: Geometry<f64> = match wkt_geom.try_into() {
      Ok(g) => g,
      Err(_) => return Err(BadRequest(
          Some("Invalid WKT in `intersects` query param. Example of a valid query: \
              ?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))".into()))),
  };

  // find our collection.  If None is returned by collections.get(), we'll return
  // none too. This will turn into a 404 error.
  let collection = match coverage.collections.get(&collection_id) {
      Some(c) => c,
      None => return Ok(None),
  };

  let imagery = collection.intersects(&bounds).as_feature_collection();
  Ok(Some(Json(to_string(&imagery).unwrap())))
}

/// STAC API Item endpoint
/// returns a GeoJSON Feature representing the item.
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/item-spec/README.md
#[get("/collections/<collection_id>/<item_id>")]
pub fn get_collection_item(
  collection_id: String,
  item_id: String,
  coverage: &State<catalog::Service>
) -> Option<Json<String>> {
  let collection = match coverage.collections.get(&collection_id) {
      Some(c) => c,
      None => return None, // becomes a 404
  };

  let item = match collection.get_item(item_id) {
      Some(i) => i,
      None => return None, // 404
  };

  Some(Json(to_string(&item.to_stac_feature()).unwrap()))
}

/// STAC API collections endpoint
/// Returns a STAC Collection JSON representation of the collection with ID `collection_id`
/// https://github.com/radiantearth/stac-api-spec/blob/master/stac-spec/collection-spec/README.md
#[get("/collections/<collection_id>")]
pub fn get_collection(collection_id: String, coverage: &State<catalog::Service>) -> Option<Json<String>> {
  let collection = match coverage.collections.get(&collection_id) {
      Some(c) => c,
      None => return None,
  };
      
  let collection = &collection.stac_collection(&coverage.base_url);
  Some(Json(to_string(collection).unwrap()))
}

/// returns a tile from a collection item covering the tile defined by its x/y/z address.
/// work in progress, will probably be removed.
#[get("/tiles/<collection_id>/<z>/<x>/<y>")]
pub fn get_tiles(collection_id: String, z: u8, x:u32, y:u32, coverage: &State<catalog::Service>) -> String {
  let bounds: Geometry<f64> = transform::to_bounds(x, y, z).try_into().unwrap();
  let collection = coverage.collections.get(&collection_id).unwrap();
  
  // currently this just returns files that could provide coverage for the tile.
  let files_for_tile = collection.intersects(&bounds);

  // stand-in for an actual tile
  format!("{} {} {} :\n {:?} :\n {:?}", z, x, y, bounds, files_for_tile)
}

/// STAC API landing page
/// based on https://github.com/radiantearth/stac-api-spec/blob/master/overview.md#example-landing-page
#[get("/")]
pub fn landing(coverage: &State<catalog::Service>) -> Json<String> {
  Json(to_string(&coverage.stac_landing()).unwrap())
}

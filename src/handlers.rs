use std::convert::TryFrom;
use std::convert::TryInto;
use std::f64;
use std::u32;
use std::u8;
use geo_types::{Geometry, Polygon};
use catalog::AsFeatureCollection;
use serde_json::{to_string};
use rocket::{State, response::content::Json};
use rocket::response::status::BadRequest;
use wkt::Wkt;
use crate::catalog::ImageIntersectsGeom;
use crate::catalog::ImageryFile;
use crate::transform;
use crate::catalog;

enum SortOrder {
  Asc,
  Desc
}

/// parse WKT supplied in a query param
fn query_to_bounds(query_str: &str) -> Result<Geometry<f64>, BadRequest<String>> {
  // convert the contains query into a Geometry.
  // WKT format is expected.
  // If any errors occur, respond to the request with a 400 error.
  let wkt_geom = match Wkt::from_str(query_str) {
    Ok(w) => w,
    Err(_) => return Err(BadRequest(
        Some("Invalid WKT in `contains` query param. Example of a valid query: \
            ?contains=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))".into()))),
  };

  let bounds: Geometry<f64> = match wkt_geom.try_into() {
      Ok(g) => g,
      Err(_) => return Err(BadRequest(
          Some("Invalid WKT in `contains` query param. Example of a valid query: \
              ?contains=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))".into()))),
    };

  Ok(bounds)
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

/// Details for a single collection.  The collection that matches `collection_id`
/// will be represented as a filtered FeatureCollection if an `intersects` or `contains` filter
/// is supplied; or if no filter supplied, a STAC Collection will be returned.
/// example:  /collections/imagery?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))
#[get("/collections/<collection_id>?<intersects>&<contains>&<sortby>&<limit>")]
pub fn get_collection(
  collection_id: String,
  intersects: Option<&str>,
  contains: Option<&str>,
  sortby: Option<&str>,
  limit: Option<usize>,
  coverage: &State<catalog::Service>,
) -> Result<Option<Json<String>>, BadRequest<String>> {

  // find our collection.  If None is returned by collections.get(), we'll return
  // none too. This will turn into a 404 error.
  let collection = match coverage.collections.get(&collection_id) {
      Some(c) => c,
      None => return Ok(None), // 404
  };

  // check if any filters were supplied. If not, return a STAC collection.
  if intersects.is_none() && contains.is_none() {
      let stac_collection = &collection.stac_collection(&coverage.base_url);
      return Ok(Some(Json(to_string(stac_collection).unwrap())));
  };

  if intersects.is_some() && contains.is_some() {
    return Err(BadRequest(Some("Use either intersects or contains, not both".into())))
  }

  let mut filtered_images: Vec<ImageryFile> = Vec::new();

  // filter on possible intersects value
  match intersects {
    Some(wkt) => {
      let bounds = query_to_bounds(wkt)?;
      filtered_images = collection.intersects(&bounds);
    },
    None => (),
  };

  // filter on possible contains value
  match contains {
    Some(wkt) => {
      let bounds = Polygon::try_from(query_to_bounds(wkt)?).unwrap();
      filtered_images = collection.contains(&bounds);
    },
    None => (),
  };

  // handle sorting.
  // currently only "spatial_resolution" is supported.
  match sortby {
    Some(s) => {
      let mut sort_key = s.trim();
      let mut ordering = SortOrder::Asc;

      // sort by ascending
      // note: Rocket parses + as whitespace.
      // however, since + (ascending) is the default, that behavior doesn't seem to affect our
      // ability to sort. This code path will only be triggered using `sortby=%2Bspatial_resolution`
      match s.strip_prefix("+") {
        Some(v) => {
          sort_key = v;
        },
        None => (),
      }

      // sort by descending
      match s.strip_prefix("-") {
        Some(v) => {
          ordering = SortOrder::Desc;
          sort_key = v;
        },
        None => (),
      }

      // hopefully a temporary measure.
      // ideally we could sort by any field of a Serde Map<String, Value> relatively
      // dynamically.
      if sort_key == "spatial_resolution" {
        let cmp = match ordering {
            SortOrder::Asc => |a: &ImageryFile, b: &ImageryFile| a.properties.resolution.avg().partial_cmp(&b.properties.resolution.avg()).unwrap(),
            SortOrder::Desc => |a: &ImageryFile, b: &ImageryFile| b.properties.resolution.avg().partial_cmp(&a.properties.resolution.avg()).unwrap(),
        } ;
        filtered_images.sort_by(cmp)
      }
      else {
        return Err(BadRequest(Some(
              "sortby currently only supports `sortby=spatial_resolution`. \
              Please file an issue to request sorting by more fields.".into()
            )))
      }     
    },
    None => (),
  }

  match limit {
    Some(lim) => {
      filtered_images = filtered_images.into_iter().take(lim).collect::<Vec<_>>();
    },
    None => (),
  }

  Ok(Some(Json(to_string(&filtered_images.as_feature_collection()).unwrap())))
}


/// returns a tile from a collection item covering the tile defined by its x/y/z address.
/// work in progress, will probably be removed.
#[get("/tiles/<collection_id>/<z>/<x>/<y>")]
pub fn get_tiles(collection_id: String, z: u8, x:u32, y:u32, coverage: &State<catalog::Service>) -> String {
  let bounds: Geometry<f64> = transform::to_bounds(x, y, z).try_into().unwrap();
  let collection = coverage.collections.get(&collection_id).unwrap();
  
  // currently this just returns files that could provide coverage for the tile.
  let files_for_tile = collection.all().intersects(&bounds);

  // stand-in for an actual tile
  format!("{} {} {} :\n {:?} :\n {:?}", z, x, y, bounds, files_for_tile)
}

/// STAC API landing page
/// based on https://github.com/radiantearth/stac-api-spec/blob/master/overview.md#example-landing-page
#[get("/")]
pub fn landing(coverage: &State<catalog::Service>) -> Json<String> {
  Json(to_string(&coverage.stac_landing()).unwrap())
}

use std::convert::TryFrom;
use std::convert::TryInto;
use std::f64;
use std::u32;
use std::u8;
use geo::polygon;
use geo_types::{Geometry, Polygon};
use catalog::AsFeatureCollection;
use rocket::http::Status;
use serde_json::{to_string};
use rocket::{State, response::content::Json};
use rocket::response::status::BadRequest;
use rocket::serde;
use wkt::Wkt;
use crate::catalog::ImageContainsPolygon;
use crate::catalog::ImageIntersectsGeom;
use crate::catalog::ImageryFile;
use crate::transform;
use crate::catalog;

enum SortOrder {
  Asc,
  Desc
}

fn bbox_to_bounds(bbox: Vec<f64>) -> Result<Geometry<f64>, BadRequest<String>> {
  if bbox.len() != 4 || bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
    return Err(BadRequest(Some("Invalid bbox. bbox must contain 4 numbers in the following format:  bbox=minx,miny,maxx,maxy".into())));
  }
  let p: Polygon<f64> = polygon![
    (x:bbox[0], y: bbox[1]),
    (x:bbox[2], y: bbox[1]),
    (x:bbox[2], y: bbox[3]),
    (x:bbox[0], y: bbox[3]),
  ];
  let g: Geometry<f64> = p.into();
  Ok(g)
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

/// preflight request for the search_all_collections POST endpoint.
#[options("/stac/search")]
pub fn search_all_preflight() -> Status {
  Status::Ok
}

/// SearchRequest represents the request body schema expected by the search_all_collections endpoint
#[derive(serde::Deserialize)]
pub struct SearchRequest {
  bbox: Option<Vec<f64>>,
  intersects: Option<String>,
  contains: Option<String>,
  sortby: Option<String>,
  limit: Option<serde::json::Value>,
}

/// search_all_collections allows searching through every collection in the catalog at once.
/// note:  much of this code is the same/similar to the collections search, and could be factored out into a
/// more modular function.
/// this endpoint works with https://github.com/sat-utils/sat-api-browser
#[post("/stac/search", data="<params>")]
pub fn search_all_collections(
  params: serde::json::Json<SearchRequest>,
  coverage: &State<catalog::Service>,
 ) -> Result<Option<Json<String>>, BadRequest<String>> {

let mut images: Vec<ImageryFile> = Vec::new();

// combine all the collections
// depending on the performance we could possibly create an index over all the collections on startup.
for (_, c) in coverage.collections.iter() {
  images.extend(c.all().to_owned())
}

// We only want to do one spatial operation. To enforce this,
// make a vec of bools representing all the possible spatial query params.
// true becomes 1 when cast to an int, so we can add up all the `trues` to make
// sure that only one (or none) was provided.
let spatial_params_mask = vec![
  params.intersects.is_some() as u8,
  params.contains.is_some() as u8,
  params.bbox.is_some() as u8
];

if spatial_params_mask.iter().sum::<u8>() > 1 {
  return Err(BadRequest(Some("Use only one of: bbox, intersects or contains".into())))
}

// filter on a bbox.
// if bbox provided, we'll always do an intersects query (instead of contains)
match &params.bbox {
  Some(b) => {
    let bounds: Polygon<f64> = bbox_to_bounds(b.to_vec())?.try_into().unwrap();
    images = images.intersects(&bounds);
  }
  None => (),
}


// filter on possible intersects value
match &params.intersects {
  Some(wkt) => {
    let bounds: Geometry<f64> = query_to_bounds(&wkt)?.try_into().unwrap();
    images = images.intersects(&bounds);
  },
  None => (),
};

// filter on possible contains value
match &params.contains {
  Some(wkt) => {
    let bounds = query_to_bounds(&wkt)?.try_into().unwrap();
    images = images.contains_polygon(&bounds);
  },
  None => (),
};

// handle sorting.
// currently only "spatial_resolution" is supported.
match &params.sortby {
  Some(s) => {
    let mut sort_key = s.trim();
    let mut ordering = SortOrder::Asc;

    // sort by ascending
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

    if sort_key == "spatial_resolution" {
      let cmp = match ordering {
          SortOrder::Asc => |a: &ImageryFile, b: &ImageryFile| a.properties.resolution.avg().partial_cmp(&b.properties.resolution.avg()).unwrap(),
          SortOrder::Desc => |a: &ImageryFile, b: &ImageryFile| b.properties.resolution.avg().partial_cmp(&a.properties.resolution.avg()).unwrap(),
      } ;
      images.sort_by(cmp)
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

// try to convert `limit` from a serde::json::Value into an integer (via a string, if necessary).
// this is here so that we can accept limit as an integer or a string (see the comments below).
// github.com/sat-utils/sat-api-browser provides the limit as a string.
match &params.limit {
  Some(v) => {
    match v {
        // limit supplied as a JSON number.  e.g. `limit: 20`
        serde_json::Value::Number(n) => {
          match n.as_u64() {
            Some(lim) => {
              images = images.into_iter().take(lim as usize).collect::<Vec<_>>();
            },
            None => (),
          }
        },

        // limit supplied as a JSON string.  e.g. `limit: "20"`
        serde_json::Value::String(s) => {
          match s.parse::<u64>() {
            Ok(lim) => {
              images = images.into_iter().take(lim as usize).collect::<Vec<_>>();
            },
            Err(_) => (),
        }
        },
        _ => ()
    };
    
  },
  None => (),
}

Ok(Some(Json(to_string(&images.as_feature_collection()).unwrap())))
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

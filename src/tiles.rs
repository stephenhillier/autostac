use std::fs::File;
use std::io;

use geo::coords_iter::CoordsIter;
use rocket::http::ContentType;
use rocket::{Request, Response, response};
use rocket::response::Responder;
use vectortile::{Encode, Feature, Layer, Tile, Value, proto};
use vectortile::grid::{Grid, Extent};
use postgis::ewkb;
use crate::catalog::ImageryFile;


struct VectorTile(File);

pub fn build_tile(features: &Vec<ImageryFile>, bbox: Extent, y: u32) -> File {
  // Build a new tile, the hard way
  let mut tile = Tile::new(&bbox);
  let mut layer = Layer::new("place");

  let p = |x, y| ewkb::Point { x: x, y: y, srid: Some(4326) };
  
  for f in features {
    // Add a new point feature "Ed's Mospresso Shack"


    let outer_ring = f.boundary.exterior();
    
    // create a polygon geometry using EWKB from the postgis crate.
    // In the future, it would be nice to find a way to encode a vector tile without the postgis crate.
    // all the current MVT implementations for Rust revolve around PostGIS.
    let points: Vec<ewkb::Point> = outer_ring.coords_iter().map(|v| p(v.x, v.y)).collect();
    let line = ewkb::LineStringT::<ewkb::Point> {srid: Some(4326), points};
    let poly: ewkb::Polygon = ewkb::PolygonT::<ewkb::Point> {srid: Some(4326), rings: vec![line]};
    let poly_geom: ewkb::GeometryT<ewkb::Point> = ewkb::GeometryT::Polygon(poly);


    let mut feature = Feature::new(poly_geom);
    feature.add_property("place", Value::String(String::from("business")));
    feature.add_property("name", Value::String(String::from("Ed's Mospresso Shack")));
    layer.add_feature(feature);
  }
  tile.add_layer(layer);


  // Encode the tile as protobuf and inspect it
  let grid = Grid::wgs84();
  let data = tile.encode(&grid);
  let mut file = File::create(y.to_string() + ".mvt").unwrap();
  data.to_writer(&mut file).unwrap();
  file
}

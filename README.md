# RS2 Raster Service

RS2 is an experimental geospatial raster data service.

It scans a directory and creates a [STAC API](https://github.com/radiantearth/stac-api-spec/blob/master/overview.md) complete
with Collections and Items generated from the objects inside.

This is a work in progress and currently the assets themselves are not served (just catalogued).

## Usage

Warning: This is a proof of concept! Use at own risk. Starting the server will attempt to open each file in the
specified directory with the `GDALOpen` function from the Georust GDAL bindings crate. If GDALOpen is unable to open
the file, it will be skipped. A STAC Item will be created for each file that GDALOpen successfully opens.


clone the repo and add some imagery to a folder:
```sh
git clone https://github.com/stephenhillier/rs2
cd rs2

# make a `data` directory under rs2
mkdir ./data

# make a subdirectory under `data`, which will be turned into a STAC Collection.
mkdir ./data/imagery

# copy some data in
cp ~/Downloads/my_image.tif ./data/imagery
```

Finally, run the server using `cargo run` and browse to http://localhost:8000/ to view the STAC API landing page.


## Goals

* catalogue spatial data (digital elevation models, satellite imagery, point clouds) in a directory tree or S3 bucket
* list spatial data available, with options to filter by a geometry/BBOX and by a date/time range
* return a dataset from either a selected file, or automatically select the best available data in a polygon. Examples:
  * give me the least cloudy image from the past month in my area of interest
  * give me the highest resolution DEM that covers `POLYGON ((...))`

## Progress
Collections are created from subdirectories, and any raster files within those subdirectories are added to their respective
collection.  The files themselves are not yet served, just catalogued.

Collections can be filtered with the `intersects` query param, which will return a FeatureCollection of STAC Item features. Example:

`http://localhost:8000/collections/imagery?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))`

### Todo list
* Error handling.
* S3 support
* Date/time search

# RS2 Raster Service

RS2 is an experimental geospatial raster data service.

It scans a directory and creates a [STAC API](https://github.com/radiantearth/stac-api-spec/blob/master/overview.md) complete
with Collections and Items generated from the objects inside.

This is a work in progress and currently the assets themselves are not served (just catalogued).

## Running the service

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

## S3

RS2 supports scanning an S3 bucket.  Within that bucket, any prefixes (subdirectories) will be turned into
collections, and any images in the bucket with prefixes will be added to their respective collections.

**WARNING**:  All files will be inspected by the GDAL vsis3 virtual filesystem driver.  Use only on a bucket
containing files you trust.

Any images in the bucket that have no prefix will be skipped (e.g.:  `/mybucket/image.tif`). Currently,
only images within subdirectories are catalogued:  `/mybucket/imagery/image.tif`.  You can make as many
subdirectories as you want, and they will all become collections.

RS2 uses the same S3 environment variables as GDAL. Example:

```sh
# Locally running Minio example
export AWS_S3_ENDPOINT=http://localhost:9000

export S3_BUCKET=mybucket
export AWS_ACCESS_KEY_ID=minio
export AWS_SECRET_ACCESS_KEY=minio123

# required for Minio - GDAL will specify the bucket in the path instead of the subdomain.
export AWS_VIRTUAL_HOSTING=FALSE 
export AWS_HTTPS=NO

cargo run -- --s3
```

## Browsing and querying the STAC API

The STAC API can be browsed by visiting the landing page at the root URL (e.g. `http://localhost:8000/`).  You can also use a STAC browser like https://github.com/radiantearth/stac-browser.

Collections will be advertised as child links from the landing page.

### Filtering collections

The collections endpoint (`/collections/<collection_id>`) supports filtering using the following query params:

**Intersects**

`intersects` takes a WKT geometry and returns imagery that intersects with any part of that query geometry.

Example:

`http://localhost:8000/collections/my_collection?intersects=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))`


**Contains**

`contains` takes a WKT geometry returns imagery that completely contains the query geometry. Note that only polygons are supported right now. Use `contains`
if you want to find an image that gives you full coverage over your area of interest.  Images may still have NoData values, cloud cover etc. over
the area of interest.

Example:

`http://localhost:8000/collections/my_collection?contains=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))`


**Filtering by date**

Todo.

### Sorting (filtered collections only)

Collections that have been filtered can also be sorted.  Currently only the `spatial_resolution` property is supported for sorting.

Collections that have not been filtered return a normal STAC collection and this will not be sorted (TODO).

Example:
`http://localhost:8000/collections/my_collection?contains=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))&sortby=spatial_resolution`

### Limit (filtered collections only)

Filtered collections that return a FeatureCollection can have a limit applied. `limit=n` will cause the FeatureCollection's Feature list
to have at most `n` features (where n is an integer).  The example below will return the highest resolution dataset that completely covers
the area of interest.

Example:

`http://localhost:8000/collections/my_collection?contains=POLYGON ((30 10, 40 40, 20 40, 10 20, 30 10))&sortby=spatial_resolution&limit=1`

## Goals

* catalogue spatial data (digital elevation models, satellite imagery, point clouds) in a directory tree or S3 bucket
* list spatial data available, with options to filter by a geometry/BBOX and by a date/time range
* return a dataset from either a selected file, or automatically select the best available data in a polygon. Examples:
  * give me the least cloudy image from the past month in my area of interest
  * give me the highest resolution DEM that covers `POLYGON ((...))`

## Progress
Collections are created from subdirectories, and any raster files within those subdirectories are added to their respective
collection.  The files themselves are not yet served, just catalogued.

Collections can be filtered with query params, which will return a FeatureCollection of STAC Item features.

### Todo list
* Date/time search
* Sort by date, resolution, cloud cover.
* Refactor catalog "backends" and add options - e.g. InMemoryCatalog, SqliteCatalog, PostGISCatalog, FileCatalog (flatgeobuf?) etc.
* Export a flat STAC catalog file

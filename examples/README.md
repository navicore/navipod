```
# you've presumably populated the db with metrics so export them
# to an N-Triples serialized RDF file.
cargo run -- export-rdf
# setup a python virtual environment
python3 -m venv venv
source ./venv/bin/activate
pip install rdflib
# run sparql queries
python ./examples/rdf_query.py
```

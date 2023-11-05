from rdflib import Graph

# Load the RDF file
g = Graph()
g.parse("navipod.ttl", format="ttl")

# Define the query
query = '''
    SELECT ?appname WHERE {
        ?s <http://navipod.navicore.tech/property/navipod_appname> ?appname .
        ?s <http://navipod.navicore.tech/property/navipod_metric_name> ?metric .
        FILTER regex(str(?metric), "jvm", "i")
    }
'''

# Execute the query
results = g.query(query)

# Print the results
for result in results:
    print(result[0])

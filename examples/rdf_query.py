from rdflib import Graph

# Load the RDF file
g = Graph()
g.parse("k8p.nt", format="nt")

# Define the query
query = '''
    SELECT DISTINCT ?appname WHERE {
        ?s <http://k8p.navicore.tech/property/k8p_appname> ?appname .
        ?s <http://k8p.navicore.tech/property/k8p_metric_name> ?metric .
        FILTER regex(str(?metric), "jvm", "i")
    }
'''

# Execute the query
results = g.query(query)

# Print the results
for result in results:
    print(result[0])

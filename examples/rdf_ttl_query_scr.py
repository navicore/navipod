from rdflib import Graph

# Load the RDF file
g = Graph()
g.parse("k8p.ttl", format="ttl")

# Define the query
query = '''
    SELECT DISTINCT ?appname WHERE {
        ?s <http://k8p.navicore.tech/property/k8p_appname> ?appname .
    }
'''

# Execute the query
results = g.query(query)

# Print the results
for result in results:
    print(result[0])

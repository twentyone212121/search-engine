import re
import requests
import argparse
import urllib.parse

def highlight_term(text, term):
    if not term:
        return text

    def escape_regex(s):
        return re.escape(s)

    words = term.split()
    escaped_term = escape_regex(term)

    exact_regex = re.compile(f'({escaped_term})', re.IGNORECASE)
    word_regexes = [re.compile(fr'\b({escape_regex(word)})\b', re.IGNORECASE) for word in words]

    highlighted_text = exact_regex.sub(r'\033[1;33m\1\033[0m', text)

    for word_regex in word_regexes:
        highlighted_text = word_regex.sub(r'\033[1;33m\1\033[0m', highlighted_text)

    return highlighted_text

class SearchClient:
    def __init__(self, server_url='http://localhost:7878'):
        self.server_url = server_url

    def search(self, query, max_results=10):
        try:
            encoded_query = urllib.parse.quote(query)

            search_response = requests.get(
                f'{self.server_url}/search?q={encoded_query}',
            )
            search_response.raise_for_status()
            search_data = search_response.json()

            if search_data['total_results'] == 0:
                print("No results found.")
                return []

            documents = []
            for result in search_data['results'][:max_results]:
                doc_response = requests.get(
                    f'{self.server_url}/document',
                    params={'docID': result['doc_id']}
                )
                doc_response.raise_for_status()
                doc = doc_response.json()
                doc['matches'] = result['matches']
                documents.append(doc)

            return documents

        except requests.RequestException as e:
            print(f"Error performing search: {e}")
            return []

    def display_results(self, query, documents):
        for i, doc in enumerate(documents, 1):
            print(f"\n--- Document {i} ---")
            print(f"Document ID: {doc['document_id']}")
            print(f"Filename: {doc['filename']}")
            print("Content:")
            highlighted_content = highlight_term(doc['content'], query)
            print(highlighted_content)
            print(f"Matches: {doc['matches']}")

def main():
    parser = argparse.ArgumentParser(description='Search Document Client')
    parser.add_argument('query', nargs='?', help='Search query')
    parser.add_argument('--url', default='http://localhost:7878',
                        help='Search server URL (default: http://localhost:7878)')

    args = parser.parse_args()

    if not args.query:
        while True:
            query = input("Enter search term (or 'quit' to exit): ").strip()
            if query.lower() == 'quit':
                break

            if query:
                client = SearchClient(args.url)
                results = client.search(query)
                client.display_results(query, results)
    else:
        client = SearchClient(args.url)
        results = client.search(args.query)
        client.display_results(args.query, results)

if __name__ == '__main__':
    main()

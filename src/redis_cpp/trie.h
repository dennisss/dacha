#ifndef TRIE_H_
#define TRIE_H_

#include <vector>


template<class T>
struct TrieEntry {
	TrieEntry() {
		value = NULL;
		children.resize(256, NULL);
	}

	~TrieEntry() {
		for(int i = 0; i < children.size(); i++) {
			if(children[i] != NULL) {
				delete children[i];
			}
		}
	}

	const T *value; // If non-null then there is a set value
	std::vector<TrieEntry<T> *> children;
};


template<class T>
class Trie {
public:

	void add(const char *key, const T *val) {
		TrieEntry<T> *e = &root;

		int i = 0;
		char c;
		while(true) {
			c = key[i];
			if(c == 0) {
				// End of string
				break;
			}

			TrieEntry<T> *ce = e->children[c];
			if(ce == NULL) {
				e->children[c] = new TrieEntry<T>();
				ce = e->children[c];
			}

			e = ce;
			i++;
		}

		e->value = val;
	}

	const T *get(const char *key, int len) {
		TrieEntry<T> *e = &root;
		
		int i = 0;
		char c;
		while(i < len) {
			c = key[i];
			/*
			if(c == 0) {
				// End of string
				break;
			}
			*/

			e = e->children[c];
			if(e == NULL) {
				return NULL;
			}

			i++;
		}

		return e->value;
	}

private: 
	TrieEntry<T> root;

};

#endif
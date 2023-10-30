
Have a general site-builder

- WE can decompose a site into pages (or directories).
- 

- Book Component
    - Mounted to a directory
    - Parameters:
        - URL subtree in which to mount the book
        - HTML template to use for the main page (how to render body and TOC left bar)
        - A 404 page template
        - A table of contents (protobuf file) : may nest to other files

How we'll use it for `dacha.page`

Directly mount as `/`


V1 would be a Rust server that I can visit to view the site (dynamically recompiled)

- Connecting to random stuff

- `/pkg/container/doc/design/node.md`

- `/code`



- `/book/api`


- `code.dacha.page`
- `edit.dacha.page`
- `api.dacha.page`


db/search
    - Inverted index mapping trigrams to doc ids.
    - Some page ranking for stuff that shows up in headings.


I'd like to have a `website_server` binary which takes as input a `WebsiteSpec`

- The spec maps routes in some priority order to handlers
- Will act sort of like nginx
- Supported handlers would be:
    - Proxy/rewrite to a backend (like an RPC server or a Cloud Storage bucket)
    - Rendering a book page
    - Genfiles (like compiled JavaScript code)
        - Technically markdown -> html could also be seen as compiled code.
- From a Website spec, we can generate a dependency tree and tell how to make a perfect bundle
    - Also nice as we may want to do stuff like 
- Other important stuff:
    - Ideally want unique ids for all files in CAS and well controlled caching

Algorithm for rendering a book page:

- Read the table of contents and follow to all files to build a set of which files are applicable.
    - Most will be markdown files
    - May also reference images (can validate during compilation that we have them all)
- If we are viewing the markdown file:
    - 

# Book Site

- Will be used as the main 


# Root Site

Domain: `dacha.page`

This is mean to mainly store documentation about the project.

'/' renders the README.md wrapped in a nice wrapper that contains links to other stuff.




For now, the root 







What I want from a personal website:

- Resume style info page
- Blog
    - Breakdown by different categories like 'teardowns'
- Catalog of References
    - e.g. books, papers, datasheets, etc.
- Eventually
    - Code Search
    - Code Editor


- Remove metadata from images with:

- `mogrify -strip ./*.jpg`
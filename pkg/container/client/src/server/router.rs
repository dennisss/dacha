use std::collections::HashMap;

use common::{errors::*, hash::FastHasherBuilder};

///
pub struct PathRouter<T> {
    routes: HashMap<String, Route<T>, FastHasherBuilder>,
}

impl<T> Default for PathRouter<T> {
    fn default() -> Self {
        Self {
            routes: HashMap::default(),
        }
    }
}

struct Route<T> {
    is_directory: bool,
    handler: T,
}

impl<T> PathRouter<T> {
    /// NOTE: 'path' is assumed to be normalized (absolute, no '..', etc.)
    pub fn add_route(&mut self, path: &str, is_directory: bool, handler: T) -> Result<()> {
        if !path.starts_with("/") {
            return Err(err_msg("Registered routes must be absolute"));
        }

        if path.ends_with("/") {
            return Err(err_msg("Route must not end in '/'"));
        }

        if let Some(_) = self.route(path) {
            return Err(err_msg("Duplicate or overlapping route"));
        }

        self.routes.insert(
            path.to_string(),
            Route {
                is_directory,
                handler,
            },
        );

        Ok(())
    }

    /// NOTE: 'path' must already be well normalized (start with '/')
    pub fn route<'a>(&'a self, path: &str) -> Option<(&'a str, &'a T)> {
        let mut selected_route = None;

        let mut path_prefix = String::new();
        path_prefix.reserve(path.len());

        let mut segments = path.split("/");
        segments.next(); // Skip the empty one before first "/"

        let mut next_segment = segments.next();
        while let Some(segment) = next_segment.take() {
            path_prefix.push('/');
            path_prefix.push_str(segment);

            next_segment = segments.next();

            if let Some((k, route)) = self.routes.get_key_value(&path_prefix) {
                if route.is_directory || next_segment.is_none() {
                    selected_route = Some((k.as_str(), &route.handler));
                    break;
                }
            }
        }

        selected_route
    }

    pub fn route_paths(&self) -> impl Iterator<Item = &str> {
        self.routes.keys().map(|s| s.as_str())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn routing_works() {
        let mut router = PathRouter::<usize>::default();

        assert_eq!(router.route("/"), None);
        assert_eq!(router.route("/hello/world"), None);

        router.add_route("/hello/world", false, 1).unwrap();
        assert_eq!(router.route("/"), None);
        assert_eq!(router.route("/hello/world"), Some(("/hello/world", &1)));
        assert_eq!(router.route("/hello/world/"), None);
        assert_eq!(router.route("/hello/world/three"), None);

        router.add_route("/dir", true, 2).unwrap();
        assert_eq!(router.route("/"), None);
        assert_eq!(router.route("/hello/world"), Some(("/hello/world", &1)));
        assert_eq!(router.route("/hello/world/"), None);
        assert_eq!(router.route("/hello/world/three"), None);
        assert_eq!(router.route("/dir"), Some(("/dir", &2)));
        assert_eq!(router.route("/dir/"), Some(("/dir", &2)));
        assert_eq!(router.route("/dir/a/b/c"), Some(("/dir", &2)));

        assert!(router.add_route("/dir/subdir", false, 2).is_err());

        router.add_route("/hello/world/three", false, 3).unwrap();
        assert_eq!(router.route("/"), None);
        assert_eq!(router.route("/hello/world"), Some(("/hello/world", &1)));
        assert_eq!(router.route("/hello/world/"), None);
        assert_eq!(
            router.route("/hello/world/three"),
            Some(("/hello/world/three", &3))
        );
        assert_eq!(router.route("/dir"), Some(("/dir", &2)));
        assert_eq!(router.route("/dir/"), Some(("/dir", &2)));
        assert_eq!(router.route("/dir/a/b/c"), Some(("/dir", &2)));
    }
}

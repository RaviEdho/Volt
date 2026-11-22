use serde::Deserialize;
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub slug: String,
    pub folder: String,
    pub web_root: String,
}

pub struct Site {
    pub host: String,
    pub document_root: PathBuf,
}

pub const MAX_PROJECTS: usize = 25;

pub fn hosts(slugs: impl IntoIterator<Item = impl AsRef<str>>) -> Result<Vec<String>, String> {
    let mut seen = HashSet::new();
    let mut hosts = Vec::new();
    for slug in slugs {
        let slug = slug.as_ref();
        let alpha_num = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
        if slug.is_empty()
            || slug.len() > 50
            || !alpha_num(slug.as_bytes()[0])
            || !alpha_num(slug.as_bytes()[slug.len() - 1])
            || !slug.bytes().all(|byte| alpha_num(byte) || byte == b'-')
        {
            return Err("Use 1–50 lowercase letters, numbers, or hyphens for the project name. Start and end with a letter or number.".into());
        }
        if !seen.insert(slug.to_owned()) {
            return Err(format!("The project name {slug} is already in use."));
        }
        if hosts.len() == MAX_PROJECTS {
            return Err(format!(
                "Volt supports up to {MAX_PROJECTS} projects in this preview."
            ));
        }
        hosts.push(format!("{slug}.test"));
    }
    Ok(hosts)
}

pub fn resolve(projects: &[Project]) -> Result<Vec<Site>, String> {
    let hosts = hosts(projects.iter().map(|project| &project.slug))?;
    projects
        .iter()
        .zip(hosts)
        .map(|(project, host)| {
            let project_path = Path::new(project.folder.trim());
            let relative = Path::new(project.web_root.trim());
            if !project_path.is_absolute() {
                return Err(format!("{host}: Enter an absolute project folder path."));
            }
            if relative.as_os_str().is_empty()
                || !relative
                    .components()
                    .all(|part| matches!(part, Component::CurDir | Component::Normal(_)))
            {
                return Err(format!(
                    "{host}: Use a relative web root inside the project, such as public or ."
                ));
            }
            let root = canonical_dir(project_path)
                .map_err(|error| format!("{host}: Could not open the project folder: {error}"))?;
            let document_root = canonical_dir(&root.join(relative))
                .map_err(|error| format!("{host}: Could not open the web root: {error}"))?;
            if !document_root.starts_with(&root) {
                return Err(format!(
                    "{host}: The web root must be inside the project folder."
                ));
            }
            Ok(Site {
                host,
                document_root,
            })
        })
        .collect()
}

fn canonical_dir(path: &Path) -> std::io::Result<PathBuf> {
    let path = dunce::canonicalize(path)?;
    if !path.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotADirectory,
            path.display().to_string(),
        ));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn hostnames_are_unique_and_safe() {
        assert_eq!(
            hosts(["blog", "my-shop"]).unwrap(),
            ["blog.test", "my-shop.test"]
        );
        for invalid in [
            "",
            "-blog",
            "blog-",
            "BLOG",
            "blog.test",
            "blog\nother.test",
            "blog'",
            "é",
        ] {
            assert!(hosts([invalid]).is_err(), "{invalid:?}");
        }
        assert!(hosts(["blog", "blog"]).is_err());
    }

    #[test]
    fn only_resolves_a_web_root_inside_the_project() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("public")).unwrap();
        let mut project = Project {
            slug: "blog".into(),
            folder: dir.path().to_str().unwrap().into(),
            web_root: "public".into(),
        };
        assert!(
            resolve(std::slice::from_ref(&project)).unwrap()[0]
                .document_root
                .ends_with("public")
        );
        for invalid in ["../", "/", "C:public", "missing"] {
            project.web_root = invalid.into();
            assert!(resolve(std::slice::from_ref(&project)).is_err());
        }
    }
}

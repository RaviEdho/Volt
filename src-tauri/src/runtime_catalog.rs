use super::Runtime;

pub struct Package {
    pub version: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub archive_root: &'static str,
}

// PHP for Windows sha256sum.txt, Apache Lounge .zip.txt/WinGet, and the
// Chocolatey MySQL recipes supply the pinned hashes. ZIPs come from the vendors.
impl Runtime {
    pub fn packages(self) -> &'static [Package] {
        match self {
            Self::Php => &[
                Package {
                    version: "8.4.28",
                    url: "https://downloads.php.net/~windows/releases/php-8.4.28-Win32-vs17-x64.zip",
                    sha256: "59ccaa095d0f0c3fbb9f7e2bfd86713ebac032d5783cc22bc8934e2285629999",
                    archive_root: "",
                },
                Package {
                    version: "8.3.37",
                    url: "https://downloads.php.net/~windows/releases/php-8.3.37-Win32-vs16-x64.zip",
                    sha256: "07984bdcfd4391888954c2e73b1cf2c0d403cfa5fd8cb32ad60ddcd854c6b7de",
                    archive_root: "",
                },
            ],
            Self::Apache => &[
                Package {
                    version: "2.4.71",
                    url: "https://www.apachelounge.com/download/VS18/binaries/httpd-2.4.71-261112-Win64-VS18.zip",
                    sha256: "44961a929dff16b767b3adbcea55760a942a00239b2891dc55806be34029e1fc",
                    archive_root: "Apache24",
                },
                Package {
                    version: "2.4.57",
                    url: "https://www.apachelounge.com/download/VS16/binaries/httpd-2.4.57-win64-VS16.zip",
                    sha256: "210b8d3610729b5da8b223ce914eaf5f553d77f8eee106b5804e6271c3ab15e2",
                    archive_root: "Apache24",
                },
            ],
            Self::Mysql => &[
                Package {
                    version: "8.4.13",
                    url: "https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip",
                    sha256: "e9cd98b1d23f122987b1afca26ad942efec5891481103d48ae3cde237fa3ea54",
                    archive_root: "mysql-8.4.13-winx64",
                },
                Package {
                    version: "8.4.6",
                    url: "https://cdn.mysql.com/archives/mysql-8.4/mysql-8.4.6-winx64.zip",
                    sha256: "b6c152f9f3aaa7294eb47db698e47974d37b261bf3cab4f90dc1243bb5ecd204",
                    archive_root: "mysql-8.4.6-winx64",
                },
            ],
        }
    }
}

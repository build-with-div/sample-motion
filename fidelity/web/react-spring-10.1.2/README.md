# react-spring 10.1.2 (vendored)

The ES module builds (`dist/react-spring_<package>.modern.mjs`) of five
react-spring packages, unmodified, one directory per package.
`shared/chunk.mjs` is the shared package's own runtime chunk, which its
module imports as `./chunk.mjs`. `LICENSE` is the packages' MIT licence;
the five packages ship byte-identical copies of it.
`tools/react_spring_oracle.py` loads them through an import map.

| Package | Version | npm integrity |
| --- | --- | --- |
| @react-spring/core | 10.1.2 | `sha512-lPGOAg0V+PV3ucopOajD+YCxoe8twALqAeG4c/+sqVjBsyUNNdx8qfz/DcNSPKA8PV3+AyQELlCxlDfap9cmBQ==` |
| @react-spring/animated | 10.1.2 | `sha512-yAsQ/bbp6+vko7WNCI1M00c6KLE9XKTGCrgRhQqS4JcK3oF5qBV4rHYrQEprvEvYXxt+1H5FsLS2eVValEPfFw==` |
| @react-spring/shared | 10.1.2 | `sha512-47/8bNQ/o0uEmxEnPBuERlC29VSqiTn5P9ln9tUMSVkYKYxBtouYE686F5kAGj+eHRPoNCw7drxWE9nv2d1LMw==` |
| @react-spring/rafz | 10.1.2 | `sha512-KC6vSFZyPnRJ2rXqipV9QqR4SaYbYXjvKfdYKWipNK2mWuV79gr20WmpVUOsTiEHGRY3WSOdWCHN+P9Pdaqb7Q==` |
| @react-spring/types | 10.1.2 | `sha512-G4CWowmVPz+rDG1y9QRVq/prZNxiNwQaHC0kgT/MgY6jAGgdRgTV4VThpvJDwWvUitk3xZB4soP4d36fjeQ09g==` |

SHA-256 of the files:

| File | SHA-256 |
| --- | --- |
| `core/react-spring_core.modern.mjs` | `80f73ea832584a10f983359e82a2ac13481cea7c605b6b09cd987cfdd6841a02` |
| `animated/react-spring_animated.modern.mjs` | `1b2cf61574ebcb2f8f5e2080aa4caaeaf3d33f1bfedc57ef3888727ac1edb041` |
| `shared/react-spring_shared.modern.mjs` | `45b4d0d0ccf93a9cda2c98cdc6f35a31513d6758e02f073bf96e06e2b83ca19b` |
| `shared/chunk.mjs` | `183552fe973134bce551888434b581d5a2cb695a82d8907e2cd9d2b61e12ce24` |
| `rafz/react-spring_rafz.modern.mjs` | `f49dc41b22af336db29c938a1f0b48e3c36fffe81cd05ab11156f0815803ed53` |
| `types/react-spring_types.modern.mjs` | `3186b08088524aa450eb231e6378d1dbe817eaa51816a8209ed716019b3431bf` |
| `LICENSE` | `51863a051d919d260e725327cb58df68d04289919d90fa64283beb9c646a2b02` |

The tarballs were fetched from `https://registry.npmjs.org/@react-spring/<package>/-/<package>-10.1.2.tgz`
and checked against the registry's `dist.integrity` before extraction. The
Rust port in `src/react_spring/` follows these modules.

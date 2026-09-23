# Changelog

## [0.32.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.31.3...fun-refactor-v0.32.0) (2026-09-23)


### Features

* bind AST definitions to text locations ([#347](https://github.com/e6qu/fun-refactor/issues/347)) ([4fdbfb8](https://github.com/e6qu/fun-refactor/commit/4fdbfb89e79ee7307dd874d51b0cfcd10a5c29f0))
* locate exact source fragments ([#352](https://github.com/e6qu/fun-refactor/issues/352)) ([e7ec8b1](https://github.com/e6qu/fun-refactor/commit/e7ec8b1d4b1799828571618f1ac13b88be14b363))
* retain text locations in agent targets ([#348](https://github.com/e6qu/fun-refactor/issues/348)) ([4e582fa](https://github.com/e6qu/fun-refactor/commit/4e582fa8199e59c8d88760169cae6bee3ca235fd))
* reveal typed source locations in Python ([#350](https://github.com/e6qu/fun-refactor/issues/350)) ([9c3121d](https://github.com/e6qu/fun-refactor/commit/9c3121dac7350dbf641f66e64521118421182429))
* type agent target locations in Python ([#349](https://github.com/e6qu/fun-refactor/issues/349)) ([a9b9692](https://github.com/e6qu/fun-refactor/commit/a9b9692a3bd084e53efe26fe6c6fd04be7c04697))


### Fixes

* bind source executable modes ([#346](https://github.com/e6qu/fun-refactor/issues/346)) ([99520d8](https://github.com/e6qu/fun-refactor/commit/99520d83b0e51f45ffdd03793f0a5567dcbce29f))
* terminate successful check descendants ([#344](https://github.com/e6qu/fun-refactor/issues/344)) ([40e030e](https://github.com/e6qu/fun-refactor/commit/40e030e26a5a440106be15bbb2fcd2e40fbd1779))
* translate bound source locations ([#351](https://github.com/e6qu/fun-refactor/issues/351)) ([b290c7d](https://github.com/e6qu/fun-refactor/commit/b290c7d5a7c357b25bcdf563c1c57d89c8eaba5a))
* verify source line ranges ([#353](https://github.com/e6qu/fun-refactor/issues/353)) ([11b8d09](https://github.com/e6qu/fun-refactor/commit/11b8d099c7cd165edb69df68e84664c356600d7f))

## [0.31.3](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.31.2...fun-refactor-v0.31.3) (2026-09-21)


### Fixes

* terminate timed-out check subprocesses ([#342](https://github.com/e6qu/fun-refactor/issues/342)) ([5dc375b](https://github.com/e6qu/fun-refactor/commit/5dc375bd53be8eb4a5beda8dc100328f534a2d55))

## [0.31.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.31.1...fun-refactor-v0.31.2) (2026-09-21)


### Performance

* cache strict spec source analysis ([#340](https://github.com/e6qu/fun-refactor/issues/340)) ([afd99a5](https://github.com/e6qu/fun-refactor/commit/afd99a5edb9bdb06facebebd7d8c225eb865d423))

## [0.31.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.31.0...fun-refactor-v0.31.1) (2026-09-21)


### Tests

* accept guided application migration ([#337](https://github.com/e6qu/fun-refactor/issues/337)) ([9805d95](https://github.com/e6qu/fun-refactor/commit/9805d95b2471c65e1bf70ec78265e270d5a64adc))

## [0.31.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.30.1...fun-refactor-v0.31.0) (2026-09-21)


### Features

* guide multi-file authored bodies ([#334](https://github.com/e6qu/fun-refactor/issues/334)) ([0b7d8ae](https://github.com/e6qu/fun-refactor/commit/0b7d8ae4057eb8050dd62e97b2185009e3f13eff))


### Tests

* accept cross-crate authored bodies ([#336](https://github.com/e6qu/fun-refactor/issues/336)) ([493041f](https://github.com/e6qu/fun-refactor/commit/493041f44db64a04fd92b80157ea4b194500355a))

## [0.30.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.30.0...fun-refactor-v0.30.1) (2026-09-21)


### Tests

* accept guided standalone CSS edit ([#331](https://github.com/e6qu/fun-refactor/issues/331)) ([30e3007](https://github.com/e6qu/fun-refactor/commit/30e30076bdf92d9cf1c711f15086508722113196))
* accept guided TSX body edit ([#333](https://github.com/e6qu/fun-refactor/issues/333)) ([660367f](https://github.com/e6qu/fun-refactor/commit/660367f26a85723770fe0e2ebee35efccb57957e))

## [0.30.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.29.0...fun-refactor-v0.30.0) (2026-09-20)


### Features

* add validated request application IR ([#321](https://github.com/e6qu/fun-refactor/issues/321)) ([252128d](https://github.com/e6qu/fun-refactor/commit/252128d96b6d71c0e7e7b20f34f21a05c00735ef))
* **application:** read validated Next.js, Express, and Go inputs ([#324](https://github.com/e6qu/fun-refactor/issues/324)) ([830b728](https://github.com/e6qu/fun-refactor/commit/830b728ee921b120672b41f3781952ce9243bce1))
* model application middleware, dependencies, service calls and component state ([#323](https://github.com/e6qu/fun-refactor/issues/323)) ([33b961e](https://github.com/e6qu/fun-refactor/commit/33b961e4789c038279a936257012d4803b70e9ac))
* read validated FastAPI application inputs ([#322](https://github.com/e6qu/fun-refactor/issues/322)) ([c93efa9](https://github.com/e6qu/fun-refactor/commit/c93efa94596f3c63d0fc9fbdb55f498daa3d873f))
* ship representative agent acceptance and SDK artifacts ([#320](https://github.com/e6qu/fun-refactor/issues/320)) ([a6c16c1](https://github.com/e6qu/fun-refactor/commit/a6c16c12e95af657328faf4b2e94952e6b261503))
* unify guided review and delivery ([#319](https://github.com/e6qu/fun-refactor/issues/319)) ([d749a88](https://github.com/e6qu/fun-refactor/commit/d749a88e7fdf4bd4c848913e72b2a8a8faee3b9e))


### Fixes

* align application audit and acceptance roadmap ([#325](https://github.com/e6qu/fun-refactor/issues/325)) ([4da7ed5](https://github.com/e6qu/fun-refactor/commit/4da7ed59bf608988464f9b2311b0b277fb771110))
* align the framework audit exclusion list with merged reader support ([06142c5](https://github.com/e6qu/fun-refactor/commit/06142c58f2a9f0c95088b422554a01e6e6f5d950))
* resolve Rust crate paths in guided upstream traces ([#326](https://github.com/e6qu/fun-refactor/issues/326)) ([7731729](https://github.com/e6qu/fun-refactor/commit/77317298e5cb505604eeff0f19bc9af574f86ec7))


### Documentation

* recount the project kernel anchors and executable cases ([0a937c3](https://github.com/e6qu/fun-refactor/commit/0a937c34b54993e8185d2de4b3b9527a36d63498))
* reset roadmap and harden agent dogfooding ([#317](https://github.com/e6qu/fun-refactor/issues/317)) ([b1e4082](https://github.com/e6qu/fun-refactor/commit/b1e40823a80041c5f738ae735bc0a557fe868019))


### Tests

* accept guided Mermaid node edit ([#330](https://github.com/e6qu/fun-refactor/issues/330)) ([295aa2c](https://github.com/e6qu/fun-refactor/commit/295aa2c841ace010b5d037e6ad8acbd9daeb4b38))
* accept guided multi-file regex rename ([#328](https://github.com/e6qu/fun-refactor/issues/328)) ([a0f55ef](https://github.com/e6qu/fun-refactor/commit/a0f55efe0824c1156c68792269e97f54b4f090fd))
* accept guided React Tailwind surface edit ([#329](https://github.com/e6qu/fun-refactor/issues/329)) ([41226d3](https://github.com/e6qu/fun-refactor/commit/41226d3bc9e93648f7035afa98b446ea1d49b1a9))

## [0.29.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.28.0...fun-refactor-v0.29.0) (2026-09-17)


### Features

* **agent:** execute reviewed guides and retain source-writing evidence ([#316](https://github.com/e6qu/fun-refactor/issues/316)) ([f0e0b71](https://github.com/e6qu/fun-refactor/commit/f0e0b7136da1fb79c8516211038d86e923ba22bc))
* complete local guide execution and matched agent evaluation ([#314](https://github.com/e6qu/fun-refactor/issues/314)) ([12ff3f2](https://github.com/e6qu/fun-refactor/commit/12ff3f2c78ac37d0e3edab75b8d4b1a486d5887a))

## [0.28.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.27.0...fun-refactor-v0.28.0) (2026-09-17)


### Features

* add generic hierarchical application transformations ([#311](https://github.com/e6qu/fun-refactor/issues/311)) ([37b35f0](https://github.com/e6qu/fun-refactor/commit/37b35f0b77529c249003cdf039227ae624f5058e))
* complete source-derived audit and guided agent validation ([#313](https://github.com/e6qu/fun-refactor/issues/313)) ([d4b8104](https://github.com/e6qu/fun-refactor/commit/d4b810469b9d5147ecdf9c0d2cfc27b0184e4a21))

## [0.27.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.26.0...fun-refactor-v0.27.0) (2026-09-16)


### Features

* add cross-language pure IR formalization and checked model correspondence ([#309](https://github.com/e6qu/fun-refactor/issues/309)) ([ef01c59](https://github.com/e6qu/fun-refactor/commit/ef01c592f7ac5bb057e26a7a24d27ec3586bedaa))

## [0.26.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.25.0...fun-refactor-v0.26.0) (2026-09-16)


### Features

* unify intent operations and agent-authored proof delivery ([#307](https://github.com/e6qu/fun-refactor/issues/307)) ([3b5b610](https://github.com/e6qu/fun-refactor/commit/3b5b610d653740c2d91f0e74f3aa0170893e85d8))

## [0.25.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.24.0...fun-refactor-v0.25.0) (2026-09-16)


### Features

* **agent:** add language-aware workflow navigation and checked delivery ([#305](https://github.com/e6qu/fun-refactor/issues/305)) ([c0669dc](https://github.com/e6qu/fun-refactor/commit/c0669dc26dd13fe4a0aeb266bba72bb3fe69d1b9))

## [0.24.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.23.0...fun-refactor-v0.24.0) (2026-09-16)


### Features

* **agent:** bind reviewed changes to intents ([#303](https://github.com/e6qu/fun-refactor/issues/303)) ([b4e5ba1](https://github.com/e6qu/fun-refactor/commit/b4e5ba115f527b26da7babc15eec93b03c724ce8))
* **agent:** compile declarative intents natively ([#301](https://github.com/e6qu/fun-refactor/issues/301)) ([27c5a84](https://github.com/e6qu/fun-refactor/commit/27c5a842db5b9140a4ea342dac4cce97e54e8803))

## [0.23.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.22.0...fun-refactor-v0.23.0) (2026-09-15)


### Features

* **agent:** add declarative structured intents ([#299](https://github.com/e6qu/fun-refactor/issues/299)) ([b60e17c](https://github.com/e6qu/fun-refactor/commit/b60e17c2e216df02a22ca96b584f1889e5c99dc1))

## [0.22.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.21.0...fun-refactor-v0.22.0) (2026-09-15)


### Features

* **agent:** add content-addressed context workspace ([#298](https://github.com/e6qu/fun-refactor/issues/298)) ([4ec9e07](https://github.com/e6qu/fun-refactor/commit/4ec9e07ad6f71b67857d7cfefa5b65613797aba8))
* **agent:** add structured Python runtime ([#297](https://github.com/e6qu/fun-refactor/issues/297)) ([68496b1](https://github.com/e6qu/fun-refactor/commit/68496b1d928c76be52fa48b620720ef59e1a746b))
* **agent:** complete reviewed change sessions ([#296](https://github.com/e6qu/fun-refactor/issues/296)) ([e6c565c](https://github.com/e6qu/fun-refactor/commit/e6c565c7ee6ea1ff97dc22d3b1dabe14aeed8264))
* close deferred agent and project boundaries ([#295](https://github.com/e6qu/fun-refactor/issues/295)) ([8d6605a](https://github.com/e6qu/fun-refactor/commit/8d6605a8f96ec7bd5285f698e15eceabfe1a9376))
* **project:** complete cross-stack agent coverage ([#293](https://github.com/e6qu/fun-refactor/issues/293)) ([b477f0f](https://github.com/e6qu/fun-refactor/commit/b477f0fb08f6f23dc5c2c8f8c191b24d2130b4c8))

## [0.21.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.20.0...fun-refactor-v0.21.0) (2026-09-14)


### Features

* **project:** disclose content-addressed project evidence ([#288](https://github.com/e6qu/fun-refactor/issues/288)) ([1f2bd22](https://github.com/e6qu/fun-refactor/commit/1f2bd224d42a3898033fcf344cb3d1acb487ff9d))
* **spec:** add agent formalization workbench ([#290](https://github.com/e6qu/fun-refactor/issues/290)) ([3ddc47e](https://github.com/e6qu/fun-refactor/commit/3ddc47e18db1e1a0af853c689d95eeefa651bef9))

## [0.20.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.19.0...fun-refactor-v0.20.0) (2026-09-14)


### Features

* **agent:** bind semantic edits to disclosure ([#285](https://github.com/e6qu/fun-refactor/issues/285)) ([3f1efc2](https://github.com/e6qu/fun-refactor/commit/3f1efc2e40d74559bd6bd2469eb505172e2631c4))

## [0.19.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.18.0...fun-refactor-v0.19.0) (2026-09-13)


### Features

* **agent:** add reviewed semantic edit plans ([#281](https://github.com/e6qu/fun-refactor/issues/281)) ([7e4981c](https://github.com/e6qu/fun-refactor/commit/7e4981cd5658f47033ef8af5d1e5fb42ae0dd967))
* **agent:** add reviewed semantic intent operations ([#280](https://github.com/e6qu/fun-refactor/issues/280)) ([bd6ffb9](https://github.com/e6qu/fun-refactor/commit/bd6ffb9b4c55353c6ce3e75c0850670c8432dbb3))
* **agent:** author checked semantic deltas ([#278](https://github.com/e6qu/fun-refactor/issues/278)) ([63d25df](https://github.com/e6qu/fun-refactor/commit/63d25df1f6b07d721ea8e4aac4eb55fa434b7755))
* **project:** add Merkle-committed progressive disclosure ([#284](https://github.com/e6qu/fun-refactor/issues/284)) ([0f5e726](https://github.com/e6qu/fun-refactor/commit/0f5e726230174525dc00cb4e7b85808b0abfbfa1))
* **project:** bound agent discovery and coalesce queries ([#283](https://github.com/e6qu/fun-refactor/issues/283)) ([4a2584f](https://github.com/e6qu/fun-refactor/commit/4a2584f1e420142daf33dd1da5a5591c6769d8fa))


### Performance

* **project:** reuse stable workspace identity ([#282](https://github.com/e6qu/fun-refactor/issues/282)) ([f371abc](https://github.com/e6qu/fun-refactor/commit/f371abc49a1e52f263502f5aca99cbc6f097d177))

## [0.18.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.17.0...fun-refactor-v0.18.0) (2026-09-12)


### Features

* **agent:** add checked semantic IR SDK ([#277](https://github.com/e6qu/fun-refactor/issues/277)) ([898a309](https://github.com/e6qu/fun-refactor/commit/898a3097bac062736ecf663bb463094e108251f0))
* **agent:** add revision-bound task bundles ([#273](https://github.com/e6qu/fun-refactor/issues/273)) ([3f020e5](https://github.com/e6qu/fun-refactor/commit/3f020e5b0815302066ca8bc0ab411ea63b2f4efd))
* **agent:** add source-free semantic model and authoring ([#276](https://github.com/e6qu/fun-refactor/issues/276)) ([12fd8d9](https://github.com/e6qu/fun-refactor/commit/12fd8d962b635ee477c3b7402296105804934f14))
* **agent:** execute reviewed task changes ([#275](https://github.com/e6qu/fun-refactor/issues/275)) ([75229aa](https://github.com/e6qu/fun-refactor/commit/75229aaa903d1ef9f6c484593c6d255b8cea9ffc))

## [0.17.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.16.0...fun-refactor-v0.17.0) (2026-09-11)


### Features

* add a verified change delivery workflow ([#271](https://github.com/e6qu/fun-refactor/issues/271)) ([a289d66](https://github.com/e6qu/fun-refactor/commit/a289d6683768831c3235e49f5bf9708d0c85cf66))
* **agent:** add context protocol v3 workflows ([#267](https://github.com/e6qu/fun-refactor/issues/267)) ([dfa01ee](https://github.com/e6qu/fun-refactor/commit/dfa01ee9e1e7ea9fd6346e425a0f62b1240b0b8f))
* **agent:** simplify high-level fr workflows ([#269](https://github.com/e6qu/fun-refactor/issues/269)) ([2a522ea](https://github.com/e6qu/fun-refactor/commit/2a522ea947eff68c5c6a65413a91ff30f7ac680f))
* **project:** batch bounded snapshot queries ([#270](https://github.com/e6qu/fun-refactor/issues/270)) ([020e1d7](https://github.com/e6qu/fun-refactor/commit/020e1d79284489b57ce6a2067db3970d1acd1eec))
* **wasm:** add checked browser transaction history ([#272](https://github.com/e6qu/fun-refactor/issues/272)) ([00dcc1d](https://github.com/e6qu/fun-refactor/commit/00dcc1dd9c37cbd10e217f60bb24ebac58353803))

## [0.16.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.15.0...fun-refactor-v0.16.0) (2026-09-10)


### Features

* add the Lean adoption kit ([#264](https://github.com/e6qu/fun-refactor/issues/264)) ([59a4525](https://github.com/e6qu/fun-refactor/commit/59a4525a2090351e3b78750940b928a778ec019c))
* **agent:** add context protocol v2 ([#261](https://github.com/e6qu/fun-refactor/issues/261)) ([0dd19b0](https://github.com/e6qu/fun-refactor/commit/0dd19b05cad78853b5ec2a5ca91554c613201c27))
* build the agent-ready verified refactoring foundation ([#259](https://github.com/e6qu/fun-refactor/issues/259)) ([31a75d7](https://github.com/e6qu/fun-refactor/commit/31a75d7800b5b44d12cc59b03b0fac6bf52bf3d3))
* complete durable Git workspace lifecycle ([#263](https://github.com/e6qu/fun-refactor/issues/263)) ([3d9a92b](https://github.com/e6qu/fun-refactor/commit/3d9a92b19f32fc27c670f6b4faeac695ae3976ed))
* generalize structural authoring ([#262](https://github.com/e6qu/fun-refactor/issues/262)) ([499d7a2](https://github.com/e6qu/fun-refactor/commit/499d7a25599e90b3e2762f41f6b488e5b2bfd899))
* **project:** add framework semantic model ([#265](https://github.com/e6qu/fun-refactor/issues/265)) ([cbd741b](https://github.com/e6qu/fun-refactor/commit/cbd741b40b002090d5783f76f8665382439ec403))
* **project:** add verified feature migration ([#266](https://github.com/e6qu/fun-refactor/issues/266)) ([a03bac5](https://github.com/e6qu/fun-refactor/commit/a03bac5d5fbd5d318d9294eed2f080c6585bb23f))

## [0.15.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.14.0...fun-refactor-v0.15.0) (2026-09-05)


### Features

* **recipe:** format recipe trees atomically ([#252](https://github.com/e6qu/fun-refactor/issues/252)) ([e17a594](https://github.com/e6qu/fun-refactor/commit/e17a594ab6b1454c563667d4e18956da4e1ea6d8))
* **recipe:** preserve formatter comment context ([#250](https://github.com/e6qu/fun-refactor/issues/250)) ([f52ee8b](https://github.com/e6qu/fun-refactor/commit/f52ee8bcbbbd0c66091401f7fe4b0686a8134451))
* **spec:** check explicit signature maps ([#255](https://github.com/e6qu/fun-refactor/issues/255)) ([1e724d0](https://github.com/e6qu/fun-refactor/commit/1e724d00f28e369ca8e7af229944072b03f16939))
* **spec:** check Lean source anchors ([#253](https://github.com/e6qu/fun-refactor/issues/253)) ([93d34f7](https://github.com/e6qu/fun-refactor/commit/93d34f73fff028eba4cf79d794954dc3cb23e210))
* **spec:** require robust signature maps ([#256](https://github.com/e6qu/fun-refactor/issues/256)) ([24f3c20](https://github.com/e6qu/fun-refactor/commit/24f3c20a2edeec8ae7ab2fb8cdea936ab47643b2))
* **spec:** sync stale Lean anchors ([#254](https://github.com/e6qu/fun-refactor/issues/254)) ([794f308](https://github.com/e6qu/fun-refactor/commit/794f3081ec0ac7d0fc705bbca97e8bbd611345fd))
* **spec:** verify strict Lean packages ([#257](https://github.com/e6qu/fun-refactor/issues/257)) ([894fc28](https://github.com/e6qu/fun-refactor/commit/894fc28a9ddfb8d0f3912c0c99b53b1996a006ac))


### Fixes

* **lean:** carry temporary collection updates ([#258](https://github.com/e6qu/fun-refactor/issues/258)) ([02b37bf](https://github.com/e6qu/fun-refactor/commit/02b37bf408e2debf74507606f6986e8f07c61962))

## [0.14.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.13.0...fun-refactor-v0.14.0) (2026-09-04)


### Features

* **recipe:** name contract steps ([#247](https://github.com/e6qu/fun-refactor/issues/247)) ([48eac0b](https://github.com/e6qu/fun-refactor/commit/48eac0bc1ef9185acfa85f17ac5484d1111a963b))

## [0.13.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.12.0...fun-refactor-v0.13.0) (2026-09-04)


### Features

* **recipe:** assert each step ([#245](https://github.com/e6qu/fun-refactor/issues/245)) ([7cd39f5](https://github.com/e6qu/fun-refactor/commit/7cd39f582de813c9d3b044383c81d1a7b9351b9d))

## [0.12.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.11.0...fun-refactor-v0.12.0) (2026-09-03)


### Features

* **recipe:** assert selected actions ([#243](https://github.com/e6qu/fun-refactor/issues/243)) ([36a31ea](https://github.com/e6qu/fun-refactor/commit/36a31ea51317ec3d2e6de478c5017a44af7c5f64))

## [0.11.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.10.0...fun-refactor-v0.11.0) (2026-09-03)


### Features

* **recipe:** replay settled renames ([#241](https://github.com/e6qu/fun-refactor/issues/241)) ([42131aa](https://github.com/e6qu/fun-refactor/commit/42131aae770b2d8935450cfd78b93004fea2d427))
* **recipe:** run recipe files as workspace transactions ([#238](https://github.com/e6qu/fun-refactor/issues/238)) ([4018362](https://github.com/e6qu/fun-refactor/commit/4018362bc8aaf7c9e8758f1901387a732f3104b7))


### Fixes

* **imports:** preserve hidden Rust import uses ([#239](https://github.com/e6qu/fun-refactor/issues/239)) ([d23c0ac](https://github.com/e6qu/fun-refactor/commit/d23c0acefa72b25308aabf83c86d4d4762d0d173))


### Performance

* **graph:** cache workspace call graphs ([#236](https://github.com/e6qu/fun-refactor/issues/236)) ([0e7aa64](https://github.com/e6qu/fun-refactor/commit/0e7aa64530efc6ba00d35920d07b88961474dcb3))
* **mentions:** cache parsed textual spans ([#240](https://github.com/e6qu/fun-refactor/issues/240)) ([2bacb32](https://github.com/e6qu/fun-refactor/commit/2bacb32f6bd61f91c8b378ac1d4b6c16e225f143))

## [0.10.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.4...fun-refactor-v0.10.0) (2026-09-03)


### Features

* **graph:** trace lexical callable values ([#235](https://github.com/e6qu/fun-refactor/issues/235)) ([3a6c1ec](https://github.com/e6qu/fun-refactor/commit/3a6c1ec23ea74632307d00fcc78fcaddd4cf5fb5))
* **inline:** substitute around literal data ([#230](https://github.com/e6qu/fun-refactor/issues/230)) ([472752d](https://github.com/e6qu/fun-refactor/commit/472752db9be05014b4cc1a8aadd91e24d4c3141c))
* **inline:** substitute resolved parameter references ([#231](https://github.com/e6qu/fun-refactor/issues/231)) ([a1b3602](https://github.com/e6qu/fun-refactor/commit/a1b3602e7f1d756ceffb95705c24d5980157aa41))


### Fixes

* **inline:** refuse parameter field-name collisions ([#227](https://github.com/e6qu/fun-refactor/issues/227)) ([caae510](https://github.com/e6qu/fun-refactor/commit/caae510099a1b72fdd07bf24dc866f9d44cf17ca))
* **inline:** refuse Rust struct shorthand ([#224](https://github.com/e6qu/fun-refactor/issues/224)) ([d049d8e](https://github.com/e6qu/fun-refactor/commit/d049d8e0e38553c573012ce2a482bbc830841ca2))
* **inline:** refuse substitutions inside character literals ([#229](https://github.com/e6qu/fun-refactor/issues/229)) ([5cfafab](https://github.com/e6qu/fun-refactor/commit/5cfafab64d42b9661b9882fc0bead52ed6a05e62))
* **inline:** refuse substitutions inside literals ([#228](https://github.com/e6qu/fun-refactor/issues/228)) ([47c53c4](https://github.com/e6qu/fun-refactor/commit/47c53c4edf55478ad54419f59d1bcc15d754a006))
* **inline:** refuse TypeScript object shorthand ([#225](https://github.com/e6qu/fun-refactor/issues/225)) ([ba24020](https://github.com/e6qu/fun-refactor/commit/ba24020679dc3f79567e053cc720f2e09aa9d406))
* **inline:** refuse TypeScript object shorthand calls ([#226](https://github.com/e6qu/fun-refactor/issues/226)) ([80f9416](https://github.com/e6qu/fun-refactor/commit/80f9416edc45945bd949bd6611853e3bd5f6f3b2))
* **lean:** resolve chained branch bindings ([#234](https://github.com/e6qu/fun-refactor/issues/234)) ([5a98387](https://github.com/e6qu/fun-refactor/commit/5a983871956ea999224e6c682b7e679558f55c26))
* **signature:** refuse changes through expansions ([#222](https://github.com/e6qu/fun-refactor/issues/222)) ([d565ec9](https://github.com/e6qu/fun-refactor/commit/d565ec923fe9db7f0f796482aceebbfb991c66a5))

## [0.9.4](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.3...fun-refactor-v0.9.4) (2026-09-01)


### Fixes

* **signature:** refuse positional changes after keywords ([#220](https://github.com/e6qu/fun-refactor/issues/220)) ([ade1077](https://github.com/e6qu/fun-refactor/commit/ade1077b6f579aaa652205904c3ecd88b58e1861))

## [0.9.3](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.2...fun-refactor-v0.9.3) (2026-09-01)


### Fixes

* **inline:** refuse shadowed closure parameters ([#218](https://github.com/e6qu/fun-refactor/issues/218)) ([4fdf63a](https://github.com/e6qu/fun-refactor/commit/4fdf63a69530a414ade40f1449fa057582bf2ee4))
* **inline:** refuse unsupported substitutions ([#219](https://github.com/e6qu/fun-refactor/issues/219)) ([6076f6a](https://github.com/e6qu/fun-refactor/commit/6076f6a52e8a5dcc7633d7072cd7c42c2d5c6709))


### Tests

* **imports:** dogfood self cleanup plan ([#213](https://github.com/e6qu/fun-refactor/issues/213)) ([db220f6](https://github.com/e6qu/fun-refactor/commit/db220f67fefd9901d4bd90790875b80f40d8feba))
* **inline:** cover mutable Rust receivers ([#216](https://github.com/e6qu/fun-refactor/issues/216)) ([d9456c6](https://github.com/e6qu/fun-refactor/commit/d9456c67cad5b83bbd27d1ada87b369e4bafb0b9))
* **inline:** cover Rust method receivers ([#215](https://github.com/e6qu/fun-refactor/issues/215)) ([09fff5f](https://github.com/e6qu/fun-refactor/commit/09fff5f9b21a92958086b7106529c44daf9c00ea))
* **inline:** cover typed Rust receivers ([#217](https://github.com/e6qu/fun-refactor/issues/217)) ([dd0dfcb](https://github.com/e6qu/fun-refactor/commit/dd0dfcbe819328d8aa0a48741d12638f8e113358))

## [0.9.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.1...fun-refactor-v0.9.2) (2026-09-01)


### Fixes

* **inline:** substitute Rust method receivers ([#212](https://github.com/e6qu/fun-refactor/issues/212)) ([1abc39a](https://github.com/e6qu/fun-refactor/commit/1abc39a1fb6cbb6cfa0d13f6644a3a6b747c7f39))


### Tests

* **kernels:** audit self extraction edits ([#209](https://github.com/e6qu/fun-refactor/issues/209)) ([741163a](https://github.com/e6qu/fun-refactor/commit/741163a59bcb09ff325dccb1c198d23af0e9ae8e))
* **kernels:** audit self inline edits ([#211](https://github.com/e6qu/fun-refactor/issues/211)) ([bc7c325](https://github.com/e6qu/fun-refactor/commit/bc7c325f75e966e896eb1b7a2c1dbc7f499a3fb4))

## [0.9.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.9.0...fun-refactor-v0.9.1) (2026-09-01)


### Tests

* **kernels:** audit self move edits ([#207](https://github.com/e6qu/fun-refactor/issues/207)) ([72892c4](https://github.com/e6qu/fun-refactor/commit/72892c4a85b0d9c5be689a4403d3a83c536965ef))

## [0.9.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.3...fun-refactor-v0.9.0) (2026-09-01)


### Features

* **kernels:** check byte-native positions ([#205](https://github.com/e6qu/fun-refactor/issues/205)) ([67be33e](https://github.com/e6qu/fun-refactor/commit/67be33edaee8e577996d7af54482f2cbbc21e857))


### Tests

* **kernels:** audit self signature edits ([#204](https://github.com/e6qu/fun-refactor/issues/204)) ([b3c9105](https://github.com/e6qu/fun-refactor/commit/b3c910504ded67be3f765205208ffff2f6066c56))
* **kernels:** dogfood edit plans on fr ([#202](https://github.com/e6qu/fun-refactor/issues/202)) ([39ff80f](https://github.com/e6qu/fun-refactor/commit/39ff80fa42e7f4185769b07b04459dbb4cefb021))

## [0.8.3](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.2...fun-refactor-v0.8.3) (2026-08-31)


### Fixes

* keep Python reassignments live ([#199](https://github.com/e6qu/fun-refactor/issues/199)) ([e169e91](https://github.com/e6qu/fun-refactor/commit/e169e9181a733c71d920f624d8896902e70485c0))

## [0.8.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.1...fun-refactor-v0.8.2) (2026-08-31)


### Tests

* name the column where a `do`-level `else` stops belonging to its own `if` ([#197](https://github.com/e6qu/fun-refactor/issues/197)) ([95f7ba9](https://github.com/e6qu/fun-refactor/commit/95f7ba9d6888794d71afd2a626d699ed754a6e9b))

## [0.8.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.8.0...fun-refactor-v0.8.1) (2026-08-31)


### Build

* a parse table has no readable diff, and saying so lets a review load ([#195](https://github.com/e6qu/fun-refactor/issues/195)) ([2803d35](https://github.com/e6qu/fun-refactor/commit/2803d353c7a53b91e438420d4dc97f7554d7a7f6))

## [0.8.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.7.0...fun-refactor-v0.8.0) (2026-08-31)


### Features

* Lean is a source as well as a target, and the two lists meet again ([#192](https://github.com/e6qu/fun-refactor/issues/192)) ([dfb4a48](https://github.com/e6qu/fun-refactor/commit/dfb4a48625a86878a63c8781dce9f542dea62bda))

## [0.7.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.6.0...fun-refactor-v0.7.0) (2026-08-31)


### Features

* Lean is a translate target, and the reader and writer lists part ([#191](https://github.com/e6qu/fun-refactor/issues/191)) ([623abea](https://github.com/e6qu/fun-refactor/commit/623abeacf3c0e9583fba1ec49d341d6099da819f))


### Documentation

* a plan for specs in Lean, and four headings that had stopped being true ([#189](https://github.com/e6qu/fun-refactor/issues/189)) ([7d4f6f4](https://github.com/e6qu/fun-refactor/commit/7d4f6f419b2f4170b9dcf17f619fec91a013823d))

## [0.6.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.5.2...fun-refactor-v0.6.0) (2026-08-30)


### Features

* Lean 4, the nineteenth language, read but not yet written ([#187](https://github.com/e6qu/fun-refactor/issues/187)) ([5c61ab9](https://github.com/e6qu/fun-refactor/commit/5c61ab9cc738bac2dabdda76a70dd9393cc385ac))

## [0.5.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.5.1...fun-refactor-v0.5.2) (2026-08-30)


### Fixes

* a group that lost its own symbol, and the sample that hid it ([#185](https://github.com/e6qu/fun-refactor/issues/185)) ([fc393a4](https://github.com/e6qu/fun-refactor/commit/fc393a4f0dabbe09b6d6f8ad6b9a07a8922ba94c))

## [0.5.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.5.0...fun-refactor-v0.5.1) (2026-08-30)


### Refactoring

* the passive goes, and so does the rule that missed most of it ([#183](https://github.com/e6qu/fun-refactor/issues/183)) ([08bb3db](https://github.com/e6qu/fun-refactor/commit/08bb3db9a2906fe2ac74a30554c4412974fe4bb5))

## [0.5.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.4.0...fun-refactor-v0.5.0) (2026-08-30)


### Features

* every target extracts a region that returns, and no comment speaks passively ([#181](https://github.com/e6qu/fun-refactor/issues/181)) ([027ec5f](https://github.com/e6qu/fun-refactor/commit/027ec5f6479942e6920de1c7804db142878812f6))

## [0.4.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.3.0...fun-refactor-v0.4.0) (2026-08-28)


### Features

* extract a region that returns, in the targets that can say it ([#178](https://github.com/e6qu/fun-refactor/issues/178)) ([565540e](https://github.com/e6qu/fun-refactor/commit/565540e69ea63a6254517191389baf18ba1feedd))

## [0.3.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.2.2...fun-refactor-v0.3.0) (2026-08-28)


### Features

* fr tells an agent what a recipe may say, and four defects it found ([#175](https://github.com/e6qu/fun-refactor/issues/175)) ([b51038d](https://github.com/e6qu/fun-refactor/commit/b51038d5d68cb2be82023eb863b6d46c14cfb078))

## [0.2.2](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.2.1...fun-refactor-v0.2.2) (2026-08-28)


### Fixes

* four defects fr found in fr ([#173](https://github.com/e6qu/fun-refactor/issues/173)) ([7ad30a1](https://github.com/e6qu/fun-refactor/commit/7ad30a1636d5e395ce908696c7ddd94fa6b3c386))

## [0.2.1](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.2.0...fun-refactor-v0.2.1) (2026-08-28)


### Fixes

* name an archive from the version, and leave the tag alone ([#172](https://github.com/e6qu/fun-refactor/issues/172)) ([e7e2a13](https://github.com/e6qu/fun-refactor/commit/e7e2a1326e777d284913ef910c61d0d44158fc54))
* the release ships all five artifacts, and names them properly ([#170](https://github.com/e6qu/fun-refactor/issues/170)) ([7045923](https://github.com/e6qu/fun-refactor/commit/704592396eb7b15b51d763818470a31ad9a2de3b))

## [0.2.0](https://github.com/e6qu/fun-refactor/compare/fun-refactor-v0.1.0...fun-refactor-v0.2.0) (2026-08-27)


### Features

* tagged releases with built binaries, and 11,413 fewer lines of comment ([#167](https://github.com/e6qu/fun-refactor/issues/167)) ([e7cd2a5](https://github.com/e6qu/fun-refactor/commit/e7cd2a5a9d2bb035f666cd3dac4f604e0460583e))


### Fixes

* the first release threw before it built anything ([#168](https://github.com/e6qu/fun-refactor/issues/168)) ([b0401d0](https://github.com/e6qu/fun-refactor/commit/b0401d0c7ede66094dd641b818d7c822a4a0d524))

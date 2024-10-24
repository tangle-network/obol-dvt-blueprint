# <h1 align="center">Obol Distributed Validator Blueprint 🌐</h1>

<h3 align="center">A Tangle Blueprint for running a Obol Distributed Validator Cluster</h3>

## 📚 Overview

This Tangle Blueprint provides a specification for running a [group](https://docs.obol.org/docs/start/quickstart_group)
Obol Distributed Validator Cluster as an <abbr title="Actively Validated Service">AVS</abbr> on the Tangle Network.

## 🚀 Features

- Automated devops for running <abbr title="Distributed Validator Technology">DVT</abbr> clusters.
- Automatically performs Obol's <abbr title="Distributed Key Generation">DKG</abbr> process
- Tangle Network integration for on-demand instancing of <abbr title="Distributed Validator Technology">DVT</abbr>
  clusters

## 🛠️ How It Works

1. **Cluster Configuration**: The blueprint defines the structure for configuring a Distributed Validator Cluster,
   including the number of operators, threshold for signing, and validator details.
2. **Leader Selection**: For simplicity, the leader is simply the first operator.
3. **Distributed Key Generation**: Automatically performs Obol's <abbr title="Distributed Key Generation">DKG</abbr>
   ceremony process
    * Each operator [creates](https://docs.obol.org/docs/charon/charon-cli-reference#creating-an-enr-for-charon)
      an <abbr title="Ethereum Node Record">ENR</abbr>, and then shares them with the leader.
    * The leader uses these <abbr title="Ethereum Node Record">ENR</abbr>s
      to [create the DKG config](https://docs.obol.org/docs/charon/charon-cli-reference#creating-the-configuration-for-a-dkg-ceremony)
    * The leader distributes the <abbr title="Distributed Key Generation">DKG</abbr> config back to the other operators
    * The [DKG ceremony](https://docs.obol.org/docs/charon/charon-cli-reference#performing-a-dkg-ceremony) starts,
      generating the cluster definition files.
4. **Tangle Integration**: Allows on-demand instancing of Obol <abbr title="Distributed Validator Technology">DVT</abbr>
   clusters using Tangle's operator set.

## 📋 Pre-requisites

* [Docker](https://docs.docker.com/engine/install/)
* [Docker Compose](https://docs.docker.com/compose/install/)
* [cargo-tangle](https://crates.io/crates/cargo-tangle)

## 💻 Usage

To use this blueprint:

1. Review the blueprint specifications in the `src/` directory.
2. Follow the [Obol documentation](https://docs.obol.org/docs/start/quickstart_group) to understand the Distributed
   Validator setup process.
3. Adapt the blueprint to your specific cluster configuration needs.
    * For simplicity, this blueprint by default will simply copy
      the [sample Holesky config](https://github.com/ObolNetwork/charon-distributed-validator-node/blob/main/.env.sample.holesky).
      This can be
      changed [here](https://github.com/tangle-network/obol-dvt-blueprint/blob/7e9f169cd84683c78e8122e3341e59aa41c2b91c/src/operator.rs#L43).
4. Deploy the blueprint on the Tangle Network using the Tangle CLI:

```shell
$ cargo tangle blueprint deploy
```

5. Activate the DV
    * See the [Obol documentation](https://docs.obol.org/docs/start/activate-dv) for this section. Once the operators
      have finished the <abbr title="Distributed Key Generation">DKG</abbr> ceremony, the `deposit-data.json` file will
      be generated, and can be taken from any of the operators.

## 🔗 External Links

- [Obol Documentation](https://docs.obol.org/)
- [Tangle Network](https://www.tangle.tools/)
- [Ethereum 2.0 Staking](https://ethereum.org/en/staking/)

## 📜 License

Licensed under either of

* Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 📬 Feedback and Contributions

We welcome feedback and contributions to improve this blueprint.
Please open an issue or submit a pull request on our GitHub repository.
Please let us know if you fork this blueprint and extend it too!

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
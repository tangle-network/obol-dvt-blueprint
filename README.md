# <h1 align="center">Obol Distributed Validator Blueprint 🌐</h1>

**A Tangle Blueprint for running a Obol Distributed Validator Cluster**

## 📚 Overview

This Tangle Blueprint provides a specification for running an Obol Distributed Validator Cluster as an Actively Validated Service (AVS) on the Tangle Network. It leverages Obol's distributed validator technology to enhance the security and reliability of Ethereum 2.0 staking operations.

## 🚀 Features

- Automated devops for running DVT clusters.
- Integration with Obol's distributed key generation (DKG) process
- Tangle Network integration for on-demand instancing of DVT clusters

## 🛠️ How It Works

1. **Cluster Configuration**: The blueprint defines the structure for configuring a Distributed Validator Cluster, including the number of operators, threshold for signing, and validator details.

2. **Distributed Key Generation**: Implements Obol's DKG ceremony process, allowing multiple operators to jointly create validator keys without any single party having full control.

3. **Node Operation**: Specifies how individual nodes in the cluster should be set up and operated, including charon client configuration and peer connectivity.

4. **Tangle Integration**: Allows on-demand instancing of Obol DVT clusters using Tangle's operator set.

## 💻 Usage

To use this blueprint:

1. Review the blueprint specifications in the `src/` directory.
2. Follow the Obol documentation to understand the Distributed Validator setup process.
3. Adapt the blueprint to your specific cluster configuration needs.
4. Deploy the blueprint on the Tangle Network using Tangle's deployment tools.

## 🔗 External Links

- [Obol Documentation](https://docs.obol.org/)
- [Tangle Network](https://www.tangle.tools/)
- [Ethereum 2.0 Staking](https://ethereum.org/en/staking/)

## 📬 Feedback and Contributions

We welcome feedback and contributions to improve this blueprint. Please open an issue or submit a pull request on our GitHub repository. Please let us know if you fork this blueprint and extend it too!

## 📜 License

This project is licensed under the MIT License. See the [LICENSE](./LICENSE) file for details.
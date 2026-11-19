// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import "forge-std/Script.sol";
import "../src/ArenaSettlement.sol";

contract DeployArenaSettlement is Script {
    function run() external returns (ArenaSettlement settlement) {
        uint256 deployerKey = vm.envUint("PRIVATE_KEY");
        address usdcAddress = vm.envAddress("USDC_ADDRESS");
        address operatorAddress = vm.envOr("OPERATOR_ADDRESS", address(0));
        uint256 treasurySeed = vm.envOr("TREASURY_SEED_USDC", uint256(0));

        vm.startBroadcast(deployerKey);

        settlement = new ArenaSettlement(usdcAddress);
        if (operatorAddress != address(0) && operatorAddress != vm.addr(deployerKey)) {
            settlement.setOperator(operatorAddress, true);
        }

        if (treasurySeed > 0) {
            IERC20Like(usdcAddress).transfer(address(settlement), treasurySeed * 1e6);
        }

        vm.stopBroadcast();
    }
}

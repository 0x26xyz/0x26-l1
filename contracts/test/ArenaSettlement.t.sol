// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

import "forge-std/Test.sol";
import "../src/ArenaSettlement.sol";
import "./MockUsdc.sol";

contract ArenaSettlementTest is Test {
    MockUsdc internal usdc;
    ArenaSettlement internal settlement;

    uint256 internal operatorPk = 0xA11CE;
    address internal operator = vm.addr(operatorPk);
    address internal player = address(0xBEEF);
    address internal otherPlayer = address(0xCAFE);

    bytes32 internal roomId = keccak256("BTC-HL-room");
    bytes32 internal symbolHash = keccak256("HYPERLIQUID:BTC");

    function setUp() public {
        vm.warp(1_800_000_000);
        usdc = new MockUsdc();
        settlement = new ArenaSettlement(address(usdc));
        settlement.setOperator(operator, true);

        usdc.mint(address(this), 10_000e6);
        usdc.mint(player, 1_000e6);
        usdc.mint(otherPlayer, 1_000e6);

        usdc.approve(address(settlement), type(uint256).max);
        vm.prank(player);
        usdc.approve(address(settlement), type(uint256).max);
        vm.prank(otherPlayer);
        usdc.approve(address(settlement), type(uint256).max);

        settlement.treasuryDeposit(5_000e6);
        settlement.registerRoom(roomId, uint64((block.timestamp - 20) * 1000), 4_000, 100e8, 0, symbolHash, 250e6);
    }

    function test_placeResolveFinalizeClaim_happyPath() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 50e6, 18_500);
        bytes32 digest = settlement.quoteDigest(player, quote);
        bytes memory sig = _sign(digest);

        vm.prank(player);
        bytes32 betId = settlement.placeBet(quote, sig);

        vm.warp(block.timestamp + 8);
        settlement.resolveColumn(roomId, quote.col, 1_095e8, 1_115e8, 1_100e8);
        bytes32[] memory ids = new bytes32[](1);
        ids[0] = betId;
        settlement.finalizeBets(ids);

        (bool resolved, bool won, uint128 payout, uint8 status) = settlement.betOutcome(betId);
        assertTrue(resolved);
        assertTrue(won);
        assertEq(payout, 92_500_000);
        assertEq(status, uint8(ArenaSettlement.BetStatus.Won));

        uint256 beforeBal = usdc.balanceOf(player);
        vm.prank(player);
        settlement.claim(betId);
        assertEq(usdc.balanceOf(player), beforeBal + payout);
    }

    function test_placeBet_rejects_reused_quote() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 50e6, 12_000);
        bytes memory sig = _sign(settlement.quoteDigest(player, quote));

        vm.prank(player);
        settlement.placeBet(quote, sig);

        vm.prank(player);
        vm.expectRevert(ArenaSettlement.QuoteAlreadyUsed.selector);
        settlement.placeBet(quote, sig);
    }

    function test_placeBet_rejects_if_quote_expired() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 50e6, 12_000);
        bytes memory sig = _sign(settlement.quoteDigest(player, quote));
        vm.warp(block.timestamp + 60);

        vm.prank(player);
        vm.expectRevert(ArenaSettlement.QuoteExpired.selector);
        settlement.placeBet(quote, sig);
    }

    function test_placeBet_rejects_if_treasury_would_be_insolvent() public {
        settlement.registerRoom(keccak256("SOL"), uint64((block.timestamp - 20) * 1000), 4_000, 100e8, 0, keccak256("HYPERLIQUID:SOL"), 9_000e6);
        bytes32 riskyRoomId = keccak256("SOL");
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(riskyRoomId) + 1, 1_000e6, 65_000);
        quote.roomId = riskyRoomId;
        bytes memory sig = _sign(settlement.quoteDigest(player, quote));

        vm.prank(player);
        vm.expectRevert(ArenaSettlement.InsufficientTreasury.selector);
        settlement.placeBet(quote, sig);
    }

    function test_treasuryWithdraw_respects_reserved_liability() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 100e6, 20_000);
        bytes memory sig = _sign(settlement.quoteDigest(player, quote));
        vm.prank(player);
        settlement.placeBet(quote, sig);

        vm.expectRevert(ArenaSettlement.InsufficientTreasury.selector);
        settlement.treasuryWithdraw(address(this), 4_901e6);
    }

    function test_finalize_marks_loser_and_claim_reverts() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 50, settlement.currentCol(roomId) + 1, 50e6, 15_000);
        bytes32 betId = _place(player, quote);

        vm.warp(block.timestamp + 8);
        settlement.resolveColumn(roomId, quote.col, 1_000e8, 1_010e8, 1_005e8);
        bytes32[] memory ids = new bytes32[](1);
        ids[0] = betId;
        settlement.finalizeBets(ids);

        (, bool won,, uint8 status) = settlement.betOutcome(betId);
        assertFalse(won);
        assertEq(status, uint8(ArenaSettlement.BetStatus.Lost));

        vm.prank(player);
        vm.expectRevert(ArenaSettlement.BetNotClaimable.selector);
        settlement.claim(betId);
    }

    function test_claim_auto_finalizes_if_column_already_resolved() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 25e6, 18_000);
        bytes32 betId = _place(player, quote);

        vm.warp(block.timestamp + 8);
        settlement.resolveColumn(roomId, quote.col, 1_090e8, 1_110e8, 1_100e8);

        vm.prank(player);
        settlement.claim(betId);

        (, bool won,, uint8 status) = settlement.betOutcome(betId);
        assertTrue(won);
        assertEq(status, uint8(ArenaSettlement.BetStatus.Claimed));
    }

    function test_pause_blocks_new_bets_but_allows_resolution_and_claim() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 25e6, 18_000);
        bytes32 betId = _place(player, quote);

        settlement.setPlaceBetPaused(true);

        ArenaSettlement.ArenaBetQuote memory blockedQuote = _quote(otherPlayer, 12, settlement.currentCol(roomId) + 1, 25e6, 16_000);
        bytes memory blockedSig = _sign(settlement.quoteDigest(otherPlayer, blockedQuote));
        vm.prank(otherPlayer);
        vm.expectRevert(ArenaSettlement.Paused.selector);
        settlement.placeBet(blockedQuote, blockedSig);

        vm.warp(block.timestamp + 8);
        settlement.resolveColumn(roomId, quote.col, 1_090e8, 1_110e8, 1_100e8);
        bytes32[] memory ids = new bytes32[](1);
        ids[0] = betId;
        settlement.finalizeBets(ids);

        vm.prank(player);
        settlement.claim(betId);
    }

    function test_non_operator_cannot_resolve() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 25e6, 18_000);
        _place(player, quote);

        vm.prank(player);
        vm.expectRevert(ArenaSettlement.Unauthorized.selector);
        settlement.resolveColumn(roomId, quote.col, 1_090e8, 1_110e8, 1_100e8);
    }

    function test_finalize_is_permissionless_once_column_resolved() public {
        ArenaSettlement.ArenaBetQuote memory quote = _quote(player, 11, settlement.currentCol(roomId) + 1, 25e6, 18_000);
        bytes32 betId = _place(player, quote);
        vm.warp(block.timestamp + 8);
        settlement.resolveColumn(roomId, quote.col, 1_090e8, 1_110e8, 1_100e8);

        bytes32[] memory ids = new bytes32[](1);
        ids[0] = betId;
        vm.prank(otherPlayer);
        settlement.finalizeBets(ids);

        (, bool won,,) = settlement.betOutcome(betId);
        assertTrue(won);
    }

    function test_column_cannot_resolve_while_still_current() public {
        uint32 current = settlement.currentCol(roomId);
        vm.expectRevert(ArenaSettlement.InvalidTiming.selector);
        settlement.resolveColumn(roomId, current, 1_090e8, 1_110e8, 1_100e8);
    }

    function _place(address bettor, ArenaSettlement.ArenaBetQuote memory quote) internal returns (bytes32) {
        bytes memory sig = _sign(settlement.quoteDigest(bettor, quote));
        vm.prank(bettor);
        return settlement.placeBet(quote, sig);
    }

    function _quote(address bettor, int32 row, uint32 col, uint128 amount, uint16 multiplierBps)
        internal
        view
        returns (ArenaSettlement.ArenaBetQuote memory)
    {
        bettor;
        uint64 colStartMs = uint64((block.timestamp - 20) * 1000) + uint64(col) * 4_000;
        uint64 deadline = colStartMs + 40_000 - 4_000;
        return ArenaSettlement.ArenaBetQuote({
            roomId: roomId,
            row: row,
            col: col,
            amount: amount,
            multiplierBps: multiplierBps,
            quoteExpiryMs: deadline,
            salt: keccak256(abi.encodePacked(bettor, row, col, amount, multiplierBps))
        });
    }

    function _sign(bytes32 digest) internal view returns (bytes memory) {
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(operatorPk, digest);
        return abi.encodePacked(r, s, v);
    }
}

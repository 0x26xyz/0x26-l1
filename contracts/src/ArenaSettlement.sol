// SPDX-License-Identifier: MIT
pragma solidity 0.8.26;

interface IERC20Like {
    function balanceOf(address owner) external view returns (uint256);
    function transfer(address to, uint256 amount) external returns (bool);
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
}

contract ArenaSettlement {
    error Unauthorized();
    error RoomAlreadyRegistered();
    error RoomNotRegistered();
    error RoomInactive();
    error InvalidQuote();
    error QuoteExpired();
    error QuoteAlreadyUsed();
    error InvalidTiming();
    error InvalidResolution();
    error ColumnAlreadyResolved();
    error ColumnNotResolved();
    error PayoutCapExceeded();
    error InsufficientTreasury();
    error BetNotFound();
    error NotBetOwner();
    error BetNotClaimable();
    error BetAlreadyClaimed();
    error Paused();
    error ZeroAddress();
    error TransferFailed();

    uint64 public constant TRAVEL_TIME_MS = 40_000;
    uint64 public constant MIN_BET_LEAD_MS = 4_000;
    uint256 private constant BPS_SCALE = 10_000;

    bytes32 private constant QUOTE_TYPEHASH = keccak256(
        "ArenaBetQuote(address settlement,uint256 chainId,address player,bytes32 roomId,int32 row,uint32 col,uint128 amount,uint16 multiplierBps,uint64 quoteExpiryMs,bytes32 salt)"
    );

    enum BetStatus {
        None,
        Open,
        Won,
        Lost,
        Claimed
    }

    struct Room {
        uint64 epochStartMs;
        uint32 colIntervalMs;
        uint128 priceStep;
        uint128 anchorPrice;
        bytes32 symbolHash;
        uint128 maxPayoutCap;
        bool active;
        bool registered;
    }

    struct ColumnResolution {
        uint128 lowPrice;
        uint128 highPrice;
        uint128 settledPrice;
        bool resolved;
    }

    struct ArenaBetQuote {
        bytes32 roomId;
        int32 row;
        uint32 col;
        uint128 amount;
        uint16 multiplierBps;
        uint64 quoteExpiryMs;
        bytes32 salt;
    }

    struct Bet {
        address player;
        bytes32 roomId;
        int32 row;
        uint32 col;
        uint128 amount;
        uint16 multiplierBps;
        uint128 payout;
        BetStatus status;
    }

    address public immutable usdc;
    address public owner;
    bool public placeBetPaused;

    uint256 public openLiability;
    uint256 public claimableLiability;

    mapping(address => bool) public operators;
    mapping(bytes32 => Room) public rooms;
    mapping(bytes32 => mapping(uint32 => ColumnResolution)) public columnResolutions;
    mapping(bytes32 => Bet) public bets;
    mapping(bytes32 => bool) public usedQuoteDigests;

    event OwnershipTransferred(address indexed previousOwner, address indexed newOwner);
    event OperatorUpdated(address indexed operator, bool enabled);
    event PlaceBetPauseUpdated(bool paused);
    event RoomRegistered(
        bytes32 indexed roomId,
        uint64 epochStartMs,
        uint32 colIntervalMs,
        uint128 priceStep,
        uint128 anchorPrice,
        bytes32 symbolHash,
        uint128 maxPayoutCap
    );
    event RoomActiveUpdated(bytes32 indexed roomId, bool active);
    event TreasuryDeposited(address indexed funder, uint256 amount);
    event TreasuryWithdrawn(address indexed to, uint256 amount);
    event ColumnResolved(
        bytes32 indexed roomId,
        uint32 indexed col,
        uint128 lowPrice,
        uint128 highPrice,
        uint128 settledPrice
    );
    event BetPlaced(
        bytes32 indexed betId,
        bytes32 indexed roomId,
        address indexed player,
        int32 row,
        uint32 col,
        uint128 amount,
        uint16 multiplierBps
    );
    event BetResolved(bytes32 indexed betId, address indexed player, bool won, uint128 payout);
    event BetClaimed(bytes32 indexed betId, address indexed player, uint128 payout);

    constructor(address usdcAddress) {
        if (usdcAddress == address(0)) revert ZeroAddress();
        usdc = usdcAddress;
        owner = msg.sender;
        operators[msg.sender] = true;
        emit OwnershipTransferred(address(0), msg.sender);
        emit OperatorUpdated(msg.sender, true);
    }

    modifier onlyOperator() {
        if (!operators[msg.sender]) revert Unauthorized();
        _;
    }

    function transferOwnership(address newOwner) external onlyOperator {
        if (newOwner == address(0)) revert ZeroAddress();
        address previousOwner = owner;
        owner = newOwner;
        operators[newOwner] = true;
        emit OwnershipTransferred(previousOwner, newOwner);
        emit OperatorUpdated(newOwner, true);
    }

    function setOperator(address operator, bool enabled) external onlyOperator {
        if (operator == address(0)) revert ZeroAddress();
        operators[operator] = enabled;
        emit OperatorUpdated(operator, enabled);
    }

    function setPlaceBetPaused(bool paused_) external onlyOperator {
        placeBetPaused = paused_;
        emit PlaceBetPauseUpdated(paused_);
    }

    function registerRoom(
        bytes32 roomId,
        uint64 epochStartMs,
        uint32 colIntervalMs,
        uint128 priceStep,
        uint128 anchorPrice,
        bytes32 symbolHash,
        uint128 maxPayoutCap
    ) external onlyOperator {
        if (rooms[roomId].registered) revert RoomAlreadyRegistered();
        if (colIntervalMs == 0 || priceStep == 0 || maxPayoutCap == 0) revert InvalidQuote();

        rooms[roomId] = Room({
            epochStartMs: epochStartMs,
            colIntervalMs: colIntervalMs,
            priceStep: priceStep,
            anchorPrice: anchorPrice,
            symbolHash: symbolHash,
            maxPayoutCap: maxPayoutCap,
            active: true,
            registered: true
        });

        emit RoomRegistered(roomId, epochStartMs, colIntervalMs, priceStep, anchorPrice, symbolHash, maxPayoutCap);
    }

    function setRoomActive(bytes32 roomId, bool active) external onlyOperator {
        Room storage room = rooms[roomId];
        if (!room.registered) revert RoomNotRegistered();
        room.active = active;
        emit RoomActiveUpdated(roomId, active);
    }

    function treasuryDeposit(uint256 amount) external {
        _safeTransferFrom(usdc, msg.sender, address(this), amount);
        emit TreasuryDeposited(msg.sender, amount);
    }

    function treasuryWithdraw(address to, uint256 amount) external onlyOperator {
        if (to == address(0)) revert ZeroAddress();
        if (amount > excessTreasury()) revert InsufficientTreasury();
        _safeTransfer(usdc, to, amount);
        emit TreasuryWithdrawn(to, amount);
    }

    function currentCol(bytes32 roomId) public view returns (uint32) {
        Room storage room = rooms[roomId];
        if (!room.registered) revert RoomNotRegistered();
        uint64 nowMs = _nowMs();
        if (nowMs <= room.epochStartMs) return 0;
        return uint32((nowMs - room.epochStartMs) / room.colIntervalMs);
    }

    function quoteDigest(address player, ArenaBetQuote calldata quote) external view returns (bytes32) {
        return _quoteDigest(player, quote);
    }

    function excessTreasury() public view returns (uint256) {
        uint256 balance = IERC20Like(usdc).balanceOf(address(this));
        uint256 reserved = openLiability + claimableLiability;
        if (balance <= reserved) return 0;
        return balance - reserved;
    }

    function placeBet(ArenaBetQuote calldata quote, bytes calldata signature) external returns (bytes32 betId) {
        if (placeBetPaused) revert Paused();

        Room storage room = rooms[quote.roomId];
        if (!room.registered) revert RoomNotRegistered();
        if (!room.active) revert RoomInactive();
        if (quote.amount == 0 || quote.multiplierBps == 0) revert InvalidQuote();
        if (_nowMs() > quote.quoteExpiryMs) revert QuoteExpired();
        if (quote.col < currentCol(quote.roomId)) revert InvalidTiming();

        bytes32 digest = _quoteDigest(msg.sender, quote);
        if (usedQuoteDigests[digest]) revert QuoteAlreadyUsed();
        if (!_isAuthorizedQuoteSigner(digest, signature)) revert Unauthorized();

        uint128 payout = uint128((uint256(quote.amount) * uint256(quote.multiplierBps)) / BPS_SCALE);
        if (payout == 0 || payout > room.maxPayoutCap) revert PayoutCapExceeded();

        _safeTransferFrom(usdc, msg.sender, address(this), quote.amount);

        uint256 nextOpenLiability = openLiability + payout;
        if (nextOpenLiability + claimableLiability > IERC20Like(usdc).balanceOf(address(this))) {
            revert InsufficientTreasury();
        }

        usedQuoteDigests[digest] = true;
        betId = keccak256(abi.encodePacked(digest));
        bets[betId] = Bet({
            player: msg.sender,
            roomId: quote.roomId,
            row: quote.row,
            col: quote.col,
            amount: quote.amount,
            multiplierBps: quote.multiplierBps,
            payout: payout,
            status: BetStatus.Open
        });
        openLiability = nextOpenLiability;

        emit BetPlaced(betId, quote.roomId, msg.sender, quote.row, quote.col, quote.amount, quote.multiplierBps);
    }

    function resolveColumn(
        bytes32 roomId,
        uint32 col,
        uint128 lowPrice,
        uint128 highPrice,
        uint128 settledPrice
    ) external onlyOperator {
        Room storage room = rooms[roomId];
        if (!room.registered) revert RoomNotRegistered();
        if (!room.active) revert RoomInactive();
        if (col >= currentCol(roomId)) revert InvalidTiming();
        if (highPrice < lowPrice) revert InvalidResolution();
        if (settledPrice < lowPrice || settledPrice > highPrice) revert InvalidResolution();

        ColumnResolution storage resolution = columnResolutions[roomId][col];
        if (resolution.resolved) revert ColumnAlreadyResolved();

        resolution.lowPrice = lowPrice;
        resolution.highPrice = highPrice;
        resolution.settledPrice = settledPrice;
        resolution.resolved = true;

        emit ColumnResolved(roomId, col, lowPrice, highPrice, settledPrice);
    }

    function finalizeBets(bytes32[] calldata betIds) external {
        for (uint256 i = 0; i < betIds.length; i++) {
            _finalizeBet(betIds[i]);
        }
    }

    function claim(bytes32 betId) external {
        Bet storage bet = bets[betId];
        if (bet.player == address(0)) revert BetNotFound();
        if (bet.player != msg.sender) revert NotBetOwner();

        if (bet.status == BetStatus.Open) {
            _finalizeBet(betId);
        }

        if (bet.status == BetStatus.Claimed) revert BetAlreadyClaimed();
        if (bet.status != BetStatus.Won) revert BetNotClaimable();

        bet.status = BetStatus.Claimed;
        claimableLiability -= bet.payout;
        _safeTransfer(usdc, msg.sender, bet.payout);
        emit BetClaimed(betId, msg.sender, bet.payout);
    }

    function betOutcome(bytes32 betId) external view returns (bool resolved, bool won, uint128 payout, uint8 status) {
        Bet storage bet = bets[betId];
        if (bet.player == address(0)) revert BetNotFound();
        bool columnResolved = columnResolutions[bet.roomId][bet.col].resolved;
        resolved = bet.status != BetStatus.Open || columnResolved;
        won = bet.status == BetStatus.Won || bet.status == BetStatus.Claimed;
        payout = won ? bet.payout : 0;
        status = uint8(bet.status);
    }

    function _finalizeBet(bytes32 betId) internal {
        Bet storage bet = bets[betId];
        if (bet.player == address(0)) revert BetNotFound();
        if (bet.status != BetStatus.Open) return;

        ColumnResolution storage resolution = columnResolutions[bet.roomId][bet.col];
        if (!resolution.resolved) revert ColumnNotResolved();

        openLiability -= bet.payout;

        if (_betWins(rooms[bet.roomId], bet.row, resolution)) {
            bet.status = BetStatus.Won;
            claimableLiability += bet.payout;
            emit BetResolved(betId, bet.player, true, bet.payout);
        } else {
            bet.status = BetStatus.Lost;
            emit BetResolved(betId, bet.player, false, 0);
        }
    }

    function _betWins(Room storage room, int32 row, ColumnResolution storage resolution) internal view returns (bool) {
        int256 rowIndex = int256(row);
        int256 start = int256(uint256(room.anchorPrice)) + rowIndex * int256(uint256(room.priceStep));
        int256 end = start + int256(uint256(room.priceStep));
        int256 low = int256(uint256(resolution.lowPrice));
        int256 high = int256(uint256(resolution.highPrice));
        return high >= start && low < end;
    }

    function _quoteDigest(address player, ArenaBetQuote calldata quote) internal view returns (bytes32) {
        return keccak256(
            abi.encode(
                QUOTE_TYPEHASH,
                address(this),
                block.chainid,
                player,
                quote.roomId,
                quote.row,
                quote.col,
                quote.amount,
                quote.multiplierBps,
                quote.quoteExpiryMs,
                quote.salt
            )
        );
    }

    function _isAuthorizedQuoteSigner(bytes32 digest, bytes calldata signature) internal view returns (bool) {
        address signer = _recover(digest, signature);
        return operators[signer];
    }

    function _recover(bytes32 digest, bytes calldata signature) internal pure returns (address signer) {
        if (signature.length != 65) revert InvalidQuote();
        bytes32 r;
        bytes32 s;
        uint8 v;
        assembly {
            r := calldataload(signature.offset)
            s := calldataload(add(signature.offset, 0x20))
            v := byte(0, calldataload(add(signature.offset, 0x40)))
        }
        if (v < 27) v += 27;
        if (v != 27 && v != 28) revert InvalidQuote();
        signer = ecrecover(digest, v, r, s);
        if (signer == address(0)) revert InvalidQuote();
    }

    function _nowMs() internal view returns (uint64) {
        return uint64(block.timestamp * 1000);
    }

    function _safeTransferFrom(address token, address from, address to, uint256 amount) internal {
        (bool success, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20Like.transferFrom.selector, from, to, amount));
        if (!success || (data.length != 0 && !abi.decode(data, (bool)))) revert TransferFailed();
    }

    function _safeTransfer(address token, address to, uint256 amount) internal {
        (bool success, bytes memory data) =
            token.call(abi.encodeWithSelector(IERC20Like.transfer.selector, to, amount));
        if (!success || (data.length != 0 && !abi.decode(data, (bool)))) revert TransferFailed();
    }
}

/*!
Non-Fungible Token implementation with JSON serialization.
*/
use near_contract_standards::non_fungible_token::metadata::TokenMetadata;
use near_contract_standards::non_fungible_token::NonFungibleToken;
use near_contract_standards::non_fungible_token::{Token, TokenId};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LazyOption, UnorderedMap, UnorderedSet,LookupMap};
use near_sdk::AccountId;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, AccountId, Balance, BorshStorageKey,
    Gas, PanicOnDefault, Promise, PromiseOrValue, PromiseResult, Timestamp,
};
use std::convert::{TryFrom, TryInto};


const MINT_FEE: Balance = 1_000_000_000_000_000_000_000_0;
const PREPARE_GAS: Gas = 1_500_000_000_000_0;
near_sdk::setup_alloc!();

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    owner_id: AccountId,
    tokens: NonFungibleToken,
    metadata: LazyOption<MeowRushContractMetadata>,
    games: UnorderedMap<String, GameMetadata>,
    tickets: UnorderedMap<TokenId, TicketMetadata>,
}

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    NonFungibleToken,
    Metadata,
    TokenMetadata,
    Enumeration,
    Approval,
    GameMetadata,
    TicketMetadata,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId, metadata: MeowRushContractMetadata) -> Self {
        assert!(!env::state_exists(), "Already initialized");
        Self {
            owner_id,
            tokens: NonFungibleToken::new(
                StorageKey::NonFungibleToken,
                AccountId::try_from(env::current_account_id()).unwrap(),
                Some(StorageKey::TokenMetadata),
                Some(StorageKey::Enumeration),
                Some(StorageKey::Approval),
            ),
            metadata: LazyOption::new(StorageKey::Metadata, Some(&metadata)),
            games: UnorderedMap::new(StorageKey::GameMetadata),
            tickets: UnorderedMap::new(StorageKey::TicketMetadata),
        }
    }
    // Add ticket info
    pub fn add_ticket_info(&mut self, game_id: String,  info: TicketInfo){
        assert!(!self.games.get(&game_id).is_none(), "This game not exist");
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "Caller {} is not owner: {}",
            env::predecessor_account_id(),
            self.owner_id
        );
        let mut game = self.games.get(&game_id).unwrap();
        assert!(game.ticket_infos.get(&info.ticket_type).is_none(), "This ticket info already exist");
        game.ticket_infos.insert(info.ticket_type.clone(), info);
        self.games.insert(&game_id, &game);
    }   

    // Edit ticket info
    pub fn edit_ticket_info(&mut self, game_id: String,  info: TicketInfo){
        assert!(!self.games.get(&game_id).is_none(), "This game not exist");
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "Caller {} is not owner: {}",
            env::predecessor_account_id(),
            self.owner_id
        );
        let mut game = self.games.get(&game_id).unwrap();
        assert!(!game.ticket_infos.get(&info.ticket_type).is_none(), "This ticket is not exist");
        game.ticket_infos.insert(info.ticket_type.clone(), info);
        self.games.insert(&game_id, &game);
    }   

    /// Create new game
    pub fn create_new_game(
        &mut self,
        game_id: String, // required,
        game_title: Option<String>,
        game_description: Option<String>,
        game_banner: Option<String>,
        ticket_types: Vec<String>,     // required, type ticket => amount
        tickets_supply: Vec<u32>,      // required
        ticket_prices: Vec<U128>,       // required, type ticket =>
        selling_start_time: Timestamp, // required
    ) {
        assert!(self.games.get(&game_id).is_none(), "This game exist");
        assert!(
            env::predecessor_account_id() == self.owner_id,
            "{}",
            format!(
                "Caller {} is not owner: {}",
                env::predecessor_account_id(),
                self.owner_id
            )
        );
        let mut ticket_infos = LookupMap::new();
        for i in 0..ticket_types.len() {
            let price: Balance = (ticket_prices[i] * 1_000_000_000_000_000_000_000_000u128 as f64)
                .round() as Balance
                + MINT_FEE;
            let ticket_info = TicketInfo {
                supply: tickets_supply[i],            // required
                ticket_type: ticket_types[i].clone(), // required,
                price: price,
                sold: 0u32,
                selling_start_time: Some(0u64),
            };
            ticket_infos.insert(ticket_types[i].clone(), ticket_info);
        }

        self.games.insert(
            &game_id.clone(),
            &GameMetadata {
                game_id,
                game_title,
                game_description,
                ticket_infos,
                game_banner,
                selling_start_time,
            },
        );
    }
    #[payable]
    pub fn buy_ticket(&mut self, game_id: String, ticket_type: String) -> Promise {
        let game = self.games.get(&game_id).unwrap();
        assert!(
            env::block_timestamp() > game.selling_start_time,
            "This game has not started selling tickets yet {}",
            game.selling_start_time
            
        );
        assert!(
            game.ticket_infos.get(&ticket_type).unwrap().sold
                < game.ticket_infos.get(&ticket_type).unwrap().supply,
            "All tickets are sold out"
        );
        assert!(
            env::attached_deposit() >= game.ticket_infos.get(&ticket_type).unwrap().price,
            "Please deposit exactly price of ticket {}. You deposit {}",
            game.ticket_infos.get(&ticket_type).unwrap().price,
            env::attached_deposit()
            
        );
        let ticket_id = format!(
            "{}.{}.{}",
            game_id,
            ticket_type,
            game.ticket_infos.get(&ticket_type).unwrap().sold
        );
        log!(
            "{}",
            format!(
                "Buy new ticket: game id: {}, ticket type: {}, ticket id: {}, price: {} YoctoNear",
                game_id,
                ticket_type,
                ticket_id,
                game.ticket_infos.get(&ticket_type).unwrap().price
            )
        );
        ex_self::nft_private_mint(
            ticket_id,
            AccountId::try_from(env::predecessor_account_id()).unwrap(),
            &env::current_account_id(),
            MINT_FEE,
            PREPARE_GAS,
        )
        .then(ex_self::check_mint(
            env::predecessor_account_id(),
            game.ticket_infos.get(&ticket_type).unwrap().price,
            &env::current_account_id(),
            0,
            5_000_000_000_000_0,
        ))
    }

    #[payable]
    pub fn check_ticket(&mut self, ticket_id: String) {
        assert_one_yocto();
        assert!(
            self.tokens.owner_by_id.get(&ticket_id) == Some(env::predecessor_account_id()),
            "You do not own the ticket {}",
            self.tokens.owner_by_id.get(&ticket_id).unwrap()
            
        );
        let mut ticket = self
            .tickets
            .get(&ticket_id)
            .unwrap_or_else(|| env::panic(b"ticket id does not exist!"));
        ticket.is_used = true;
        self.tickets.insert(&ticket_id, &ticket);
        log!("{}", format!("Ticket {} is checked", ticket_id));
    }
    #[payable]
    #[private]
    pub fn nft_private_mint(&mut self, token_id: TokenId, receiver_id: AccountId) -> Token {
        let token_id_split: Vec<&str> = token_id.split(".").collect();
        let game_id = token_id_split[0].to_string();
        let ticket_type = token_id_split[1].to_string();
        let mut game = self.games.get(&game_id).unwrap();
        let mut ticket_info = game.ticket_infos.get(&ticket_type).unwrap().clone();
        ticket_info.sold += 1;
        game.ticket_infos.insert(ticket_type.clone(), ticket_info);
        self.games.insert(&game_id, &game);
        self.tickets.insert(
            &token_id,
            &TicketMetadata {
                ticket_id: token_id.clone(),
                game_id,
                ticket_type,
                is_used: false,
                issued_at: env::block_timestamp(),
                game: None,
            },
        );
        self.tokens.mint(
            token_id,
            receiver_id,
            Some(TokenMetadata {
                title: Some("MeowRush".to_string()),
                description: Some("MeowRush ticket".to_string()), 
                media: Some("".to_string()), 
                media_hash: None, 
                issued_at: Some(env::block_timestamp().to_string()), // ISO 8601 datetime when token was issued or minted
                expires_at: None,     // ISO 8601 datetime when token expires
                starts_at: None,      // ISO 8601 datetime when token starts being valid
                updated_at: None,     // ISO 8601 datetime when token was last updated
            }),
        )
    }

    pub fn check_mint(&self, buyer: AccountId, price: Balance) {
        let mut result: bool = true;
        for i in 0..env::promise_results_count() {
            if env::promise_result(i) == PromiseResult::Failed {
                result = false;
                break;
            }
        }
        if result == false {
            log!("Fail to create new ticket contract");
            Promise::new(buyer).transfer(price);
        }
    }

    pub fn get_active_games(&self) -> Vec<GameMetadata> {
        self.games
            .values()
            .filter_map(|game| {
                if game.selling_start_time < env::block_timestamp()
                {
                    Some(game)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_all_games(&self) -> Vec<GameMetadata> {
        self.games.values().collect()
    }

    pub fn game_metadata(&self, game_id: String) -> GameMetadata {
        self.games.get(&game_id).unwrap()
    }

    pub fn ticket_metadata(&self, token_id: TokenId) -> TicketMetadata {
        let mut _ticket = self.tickets.get(&token_id).unwrap();
        _ticket.game = self.games.get(&_ticket.game_id);
        _ticket
    }

}

near_contract_standards::impl_non_fungible_token_core!(Contract, tokens);
near_contract_standards::impl_non_fungible_token_approval!(Contract, tokens);
near_contract_standards::impl_non_fungible_token_enumeration!(Contract, tokens);

#[near_bindgen]
impl Contract {
    pub fn ticket_contract_metadata(&self) -> MeowRushContractMetadata {
        self.metadata.get().unwrap()
    }
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct MeowRushContractMetadata {
    pub spec: String,   // required, essentially a version like "nft-1.0.0"
    pub name: String,   // required, ex. "Mosaics"
    pub symbol: String, // required, ex. "MOSIAC"
    pub description: Option<String>,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct TicketMetadata {
    pub ticket_id: String,   // required
    pub game_id: String,     // required,
    pub ticket_type: String, // required,
    pub is_used: bool,       // required,
    issued_at: Timestamp,
    pub game: Option<GameMetadata>, // required
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct TicketInfo {
    pub supply: u32,         // required
    pub ticket_type: String, // required,
    pub price: Balance,
    pub sold: u32,
    pub selling_start_time: Option<Timestamp>,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub struct GameMetadata {
    pub game_id: String, // required,
    pub game_title: Option<String>,
    pub game_description: Option<String>,
    pub ticket_infos: LookupMap<String, TicketInfo>,
    pub game_banner: Option<String>,
    pub selling_start_time: Timestamp, // required
}

#[ext_contract(ex_self)]
trait TTicketContract {
    fn nft_private_mint(&mut self, token_id: TokenId, receiver_id: AccountId) -> Token;
    fn check_mint(&self, buyer: AccountId, price: Balance);
}

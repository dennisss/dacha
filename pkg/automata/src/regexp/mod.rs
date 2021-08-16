mod alphabet;
mod encoding;
mod instance;
mod r#match;
mod node;
mod rune;
mod state_machine;
mod symbol;
mod syntax;
pub mod vm;

pub use self::instance::RegExp;

/*
    PCRE Style RegExp Parser

    Grammar rules derived from: https://github.com/bkiers/pcre-parser/blob/master/src/main/antlr4/nl/bigo/pcreparser/PCRE.g4
*/

/*
    Annotate all state ids which correspond to a group
    - Once we hit an acceptor for one of these, then we can close the group

*/

/*
    Representing capture groups:
    - The only complication is alternation:
        - We must pretty much record the list of all states in each branch of an alternation
        - Then after minimization, compare all lists of states and figure out how to differentiate the cases
            -> Based on this we should be able to

    - Computing shortest string:
        -> Basically remove epsilons and compute graph shortest path
        -> If we can check for cycles then we tell whether or not a language is finite or infinite

    Optimization:
        - When observing a long sequence of chained characters, we can optimize the problem into:
            - https://en.wikipedia.org/wiki/Knuth%E2%80%93Morris%E2%80%93Pratt_algorithm
            - Basically the same idea as automata but in a rather condensed format

    More ideas on how to implement capture groups:
    - https://stackoverflow.com/questions/28941425/capture-groups-using-dfa-based-linear-time-regular-expressions-possible
*/

/*
    Character classes:
    -> [abcdef]
        -> For now, regular ones can be split
        -> We must make every single character class totally orthogonal
*/

/*
    We will ideally be able to reduce everything to

    Given a character class such as:
        '[aa]'
        -> Deduplication will be handled by the FSM
        -> But if we have '[ab\w]'
            -> Then we do need to perform splitting of to

*/

// TODO: What would '[\s-\w]' mean

// NOTE: This will likely end up being the token data for our state machine
// NOTE: We will also need to be able to represent inverses in the character
// sets Otherwise, we will need to make it a closed set ranges up to the maximum
// value
/*
    Given a sorted set of all possible alphabetic characters
    - We can choose exactly one in log(n) time where n is the size of the alphabet observed in the regex

*/

/*
    Given a list of all characters, we will in most cases perform two operations to look up a symbol id for a given symbol:
    ->

    -> General idea of subset decimation:
        -> Start out with the set of all symbols:
        ->

    -> Future optimizations:
        -> If a node has all or almost all possible symbols as edges out of it, it is likely gonna be more efficient to perform a method of negation comparison rather than inclusion
            -> Mainly to save on memory storing the transition table


    So we will have two internal types of regexps:
    -> One full ast tree

    -> then another which excludes custom character classes and non-well defined character classes
    -> We will Want to reduce all non-direct value sets to ranges or other types of closures

*/
/*
    Implementing ^ and $.

    - Without these, DFS should start and end with accepting
    - Every state goes back to itself on the 'start' symbol.

    - After one transition, we should know if it is possible to get from the current node to an acceptance state.
    - We should also know after one transition if everything will be an acceptance.
*/

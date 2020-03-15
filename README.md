Just doing some actors, nothing to see here.

#### TODO

- `sender.schedule(tag, envelope, delay)` to scheduler sending of a message
  - add `memory.tasks: Vec<Task>`
  - add `Action::Delay`
  - in worker thread, send `Action::Delay` for `sender.schedule`
  - add `scheduler.tasks: PriorityQueue<Task>`
  - store tasks from `Action::Delay` to `scheduler.tasks` in the main loop
  - after processing N messages on the main loop:
    - check if any `scheuler.tasks` are ready to be fired
    - if yes, put respective `Event::Queue` into the channel for each event
- concept of a `System` (actor system) ?
- IO-bridge
  - mio poll with connections as actors
  - run next to the main event loop?
  - async TCP/UDP servers expressed via actors
  - (?) web-socket message-passing friendly
- Remote-bridge
  - based on IO-bridge
  - effortless and location transparent remote communications between actors
    - (de)serialization?
    - basic "type boundaries" to make sure messages are (de)serializable
  - target address is resolved by scheduler
    - messages is sent to respective bridge/connection actor
  - receiving from remote actors
    - address is resolved when message is received
  - deterministic decentralized routing across distributed actor system
    - consistent hashing + gossip membership
    - (?) distributed hashtable
- Stop an actor
- Failure handling
  - "catch" panic from actor
- Hierarchy? - related for failure handling
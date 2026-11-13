//! Typed conversions between inert subtype vocabularies.
use crate::mtg_types::*;

#[inline]
pub fn creature_type(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Advisor
        | CreatureType::Aetherborn
        | CreatureType::Alien
        | CreatureType::Ally
        | CreatureType::Andorian
        | CreatureType::Angel
        | CreatureType::Antelope
        | CreatureType::Ape
        | CreatureType::Archer
        | CreatureType::Archon
        | CreatureType::Armadillo
        | CreatureType::Army
        | CreatureType::Artificer
        | CreatureType::Assassin
        | CreatureType::AssemblyWorker
        | CreatureType::Astartes
        | CreatureType::Atog
        | CreatureType::Aurochs
        | CreatureType::Avatar
        | CreatureType::Azra
        | CreatureType::Badger
        | CreatureType::Balloon
        | CreatureType::Barbarian
        | CreatureType::Bard
        | CreatureType::Basilisk
        | CreatureType::Bat
        | CreatureType::Bear
        | CreatureType::Beast
        | CreatureType::Beaver
        | CreatureType::Beeble
        | CreatureType::Beholder
        | CreatureType::Berserker
        | CreatureType::Bird
        | CreatureType::Bison
        | CreatureType::Blinkmoth
        | CreatureType::Boar
        | CreatureType::Borg
        | CreatureType::Bringer
        | CreatureType::Brushwagg
        | CreatureType::Caitian
        | CreatureType::Camarid
        | CreatureType::Camel
        | CreatureType::Capybara
        | CreatureType::Caribou
        | CreatureType::Carrier
        | CreatureType::Cat
        | CreatureType::Centaur
        | CreatureType::Chimera
        | CreatureType::Citizen
        | CreatureType::Cleric
        | CreatureType::Clown
        | CreatureType::Cockatrice
        | CreatureType::Construct
        | CreatureType::Coward
        | CreatureType::Coyote
        | CreatureType::Crab
        | CreatureType::Crocodile
        | CreatureType::Ctan
        | CreatureType::Custodes
        | CreatureType::Cyberman
        | CreatureType::Cyclops
        | CreatureType::Dalek
        | CreatureType::Dauthi
        | CreatureType::Demigod
        | CreatureType::Demon
        | CreatureType::Deserter
        | CreatureType::Detective
        | CreatureType::Devil
        | CreatureType::Dinosaur
        | CreatureType::Djinn
        | CreatureType::Doctor
        | CreatureType::Dog
        | CreatureType::Dragon
        | CreatureType::Drake
        | CreatureType::Dreadnought
        | CreatureType::Drix
        | CreatureType::Drone
        | CreatureType::Druid
        | CreatureType::Dryad
        | CreatureType::Dwarf
        | CreatureType::Echidna
        | CreatureType::Efreet
        | CreatureType::Egg
        | CreatureType::Elder
        | CreatureType::Eldrazi
        | CreatureType::Elemental
        | CreatureType::Elephant
        | CreatureType::Elf
        | CreatureType::Elk
        | CreatureType::Employee
        | CreatureType::Eternal
        | CreatureType::Eye
        | CreatureType::Faerie
        | CreatureType::Ferret
        | CreatureType::Fish
        | CreatureType::Flagbearer
        | CreatureType::Fox
        | CreatureType::Fractal
        | CreatureType::Frog
        | CreatureType::Fungus
        | CreatureType::Gamer
        | CreatureType::Gamma
        | CreatureType::Gargoyle
        | CreatureType::Germ
        | CreatureType::Giant
        | CreatureType::Giraffe
        | CreatureType::Gith
        | CreatureType::Glimmer
        | CreatureType::Gnoll
        | CreatureType::Gnome => creature_type_case_0(value),
        _ => creature_type_dispatch_1(value),
    }
}

#[inline]
fn creature_type_dispatch_1(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Goat
        | CreatureType::Goblin
        | CreatureType::God
        | CreatureType::Golem
        | CreatureType::Gorgon
        | CreatureType::Gorn
        | CreatureType::Graveborn
        | CreatureType::Gremlin
        | CreatureType::Griffin
        | CreatureType::Guest
        | CreatureType::Hag
        | CreatureType::Halfling
        | CreatureType::Hamster
        | CreatureType::Harpy
        | CreatureType::Hedgehog
        | CreatureType::Hellion
        | CreatureType::Hero
        | CreatureType::Hippo
        | CreatureType::Hippogriff
        | CreatureType::Homarid
        | CreatureType::Homunculus
        | CreatureType::Horror
        | CreatureType::Horse
        | CreatureType::Human
        | CreatureType::Hydra
        | CreatureType::Hyena
        | CreatureType::Illusion
        | CreatureType::Imp
        | CreatureType::Incarnation
        | CreatureType::Inhuman
        | CreatureType::Inkling
        | CreatureType::Inquisitor
        | CreatureType::Insect
        | CreatureType::Jackal
        | CreatureType::Jellyfish
        | CreatureType::Juggernaut
        | CreatureType::Kangaroo
        | CreatureType::Kavu
        | CreatureType::Kelpien
        | CreatureType::Kirin
        | CreatureType::Kithkin
        | CreatureType::Klingon
        | CreatureType::Knight
        | CreatureType::Kobold
        | CreatureType::Kor
        | CreatureType::Kraken
        | CreatureType::Kree
        | CreatureType::Lamia
        | CreatureType::Lammasu
        | CreatureType::Lanthanite
        | CreatureType::Leech
        | CreatureType::Lemur
        | CreatureType::Leviathan
        | CreatureType::Lhurgoyf
        | CreatureType::Licid
        | CreatureType::Lizard
        | CreatureType::Llama
        | CreatureType::Lobster
        | CreatureType::Manticore
        | CreatureType::Masticore
        | CreatureType::Mercenary
        | CreatureType::Merfolk
        | CreatureType::Metathran
        | CreatureType::Minion
        | CreatureType::Minotaur
        | CreatureType::Mite
        | CreatureType::Mole
        | CreatureType::Monger
        | CreatureType::Mongoose
        | CreatureType::Monk
        | CreatureType::Monkey
        | CreatureType::Moogle
        | CreatureType::Moonfolk
        | CreatureType::Mount
        | CreatureType::Mouse
        | CreatureType::Mutant
        | CreatureType::Myr
        | CreatureType::Mystic
        | CreatureType::Nautilus
        | CreatureType::Necron
        | CreatureType::Nephilim
        | CreatureType::Nightmare
        | CreatureType::Nightstalker
        | CreatureType::Ninja
        | CreatureType::Noble
        | CreatureType::Noggle
        | CreatureType::Nomad
        | CreatureType::Nymph
        | CreatureType::Octopus
        | CreatureType::Officer
        | CreatureType::Ogre
        | CreatureType::Ooze
        | CreatureType::Orb
        | CreatureType::Orc
        | CreatureType::Orgg
        | CreatureType::Orion
        | CreatureType::Otter
        | CreatureType::Ouphe
        | CreatureType::Ox
        | CreatureType::Oyster
        | CreatureType::Pangolin
        | CreatureType::Peasant
        | CreatureType::Pegasus
        | CreatureType::Pentavite
        | CreatureType::Performer
        | CreatureType::Pest
        | CreatureType::Phelddagrif
        | CreatureType::Phoenix
        | CreatureType::Phyrexian
        | CreatureType::Pilot => creature_type_case_1(value),
        _ => creature_type_dispatch_2(value),
    }
}

#[inline]
fn creature_type_dispatch_2(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Pincher
        | CreatureType::Pirate
        | CreatureType::Plant
        | CreatureType::Platypus
        | CreatureType::Porcupine
        | CreatureType::Possum
        | CreatureType::Praetor
        | CreatureType::Primarch
        | CreatureType::Prism
        | CreatureType::Processor
        | CreatureType::Q
        | CreatureType::Qu
        | CreatureType::Rabbit
        | CreatureType::Raccoon
        | CreatureType::Ranger
        | CreatureType::Rat
        | CreatureType::Rebel
        | CreatureType::Reflection
        | CreatureType::Rhino
        | CreatureType::Rigger
        | CreatureType::Robot
        | CreatureType::Rogue
        | CreatureType::Romulan
        | CreatureType::Sable
        | CreatureType::Salamander
        | CreatureType::Samurai
        | CreatureType::Sand
        | CreatureType::Saproling
        | CreatureType::Satyr
        | CreatureType::Scarecrow
        | CreatureType::Scientist
        | CreatureType::Scion
        | CreatureType::Scorpion
        | CreatureType::Scout
        | CreatureType::Sculpture
        | CreatureType::Seal
        | CreatureType::Serf
        | CreatureType::Serpent
        | CreatureType::Servo
        | CreatureType::Shade
        | CreatureType::Shaman
        | CreatureType::Shapeshifter
        | CreatureType::Shark
        | CreatureType::Sheep
        | CreatureType::Shiar
        | CreatureType::Siren
        | CreatureType::Skeleton
        | CreatureType::Skrull
        | CreatureType::Skunk
        | CreatureType::Slith
        | CreatureType::Sliver
        | CreatureType::Sloth
        | CreatureType::Slug
        | CreatureType::Snail
        | CreatureType::Snake
        | CreatureType::Soldier
        | CreatureType::Soltari
        | CreatureType::Sorcerer
        | CreatureType::Spawn
        | CreatureType::Specter
        | CreatureType::Spellshaper
        | CreatureType::Sphinx
        | CreatureType::Spider
        | CreatureType::Spike
        | CreatureType::Spirit
        | CreatureType::Splinter
        | CreatureType::Sponge
        | CreatureType::Spy
        | CreatureType::Squid
        | CreatureType::Squirrel
        | CreatureType::Starfish
        | CreatureType::Surrakar
        | CreatureType::Survivor
        | CreatureType::Symbiote
        | CreatureType::Synth
        | CreatureType::Talosian
        | CreatureType::Tellarite
        | CreatureType::Tentacle
        | CreatureType::Tetravite
        | CreatureType::Thalakos
        | CreatureType::Tholian
        | CreatureType::Thopter
        | CreatureType::Thrull
        | CreatureType::Tiefling
        | CreatureType::TimeLord
        | CreatureType::Tosk
        | CreatureType::Toy
        | CreatureType::Treefolk
        | CreatureType::Trilobite
        | CreatureType::Triskelavite
        | CreatureType::Troll
        | CreatureType::Turtle
        | CreatureType::Tyranid
        | CreatureType::Unicorn
        | CreatureType::Utrom
        | CreatureType::Vampire
        | CreatureType::Varmint
        | CreatureType::Vedalken
        | CreatureType::Villain
        | CreatureType::Volver
        | CreatureType::Vorta
        | CreatureType::Vulcan
        | CreatureType::Wall
        | CreatureType::Walrus
        | CreatureType::Warlock
        | CreatureType::Warrior
        | CreatureType::Weasel
        | CreatureType::Weird
        | CreatureType::Werewolf
        | CreatureType::Whale => creature_type_case_2(value),
        _ => creature_type_dispatch_3(value),
    }
}

#[inline]
fn creature_type_dispatch_3(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Wizard
        | CreatureType::Wolf
        | CreatureType::Wolverine
        | CreatureType::Wombat
        | CreatureType::Worm
        | CreatureType::Wraith
        | CreatureType::Wurm
        | CreatureType::Xindi
        | CreatureType::Yeti
        | CreatureType::Zombie
        | CreatureType::Zubera => creature_type_case_3(value),
        _ => unreachable!("all current variants were dispatched"),
    }
}

fn creature_type_case_0(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Advisor => SubType::Advisor,
        CreatureType::Aetherborn => SubType::Aetherborn,
        CreatureType::Alien => SubType::Alien,
        CreatureType::Ally => SubType::Ally,
        CreatureType::Andorian => SubType::Andorian,
        CreatureType::Angel => SubType::Angel,
        CreatureType::Antelope => SubType::Antelope,
        CreatureType::Ape => SubType::Ape,
        CreatureType::Archer => SubType::Archer,
        CreatureType::Archon => SubType::Archon,
        CreatureType::Armadillo => SubType::Armadillo,
        CreatureType::Army => SubType::Army,
        CreatureType::Artificer => SubType::Artificer,
        CreatureType::Assassin => SubType::Assassin,
        CreatureType::AssemblyWorker => SubType::AssemblyWorker,
        CreatureType::Astartes => SubType::Astartes,
        CreatureType::Atog => SubType::Atog,
        CreatureType::Aurochs => SubType::Aurochs,
        CreatureType::Avatar => SubType::Avatar,
        CreatureType::Azra => SubType::Azra,
        CreatureType::Badger => SubType::Badger,
        CreatureType::Balloon => SubType::Balloon,
        CreatureType::Barbarian => SubType::Barbarian,
        CreatureType::Bard => SubType::Bard,
        CreatureType::Basilisk => SubType::Basilisk,
        CreatureType::Bat => SubType::Bat,
        CreatureType::Bear => SubType::Bear,
        CreatureType::Beast => SubType::Beast,
        CreatureType::Beaver => SubType::Beaver,
        CreatureType::Beeble => SubType::Beeble,
        CreatureType::Beholder => SubType::Beholder,
        CreatureType::Berserker => SubType::Berserker,
        CreatureType::Bird => SubType::Bird,
        CreatureType::Bison => SubType::Bison,
        CreatureType::Blinkmoth => SubType::Blinkmoth,
        CreatureType::Boar => SubType::Boar,
        CreatureType::Borg => SubType::Borg,
        CreatureType::Bringer => SubType::Bringer,
        CreatureType::Brushwagg => SubType::Brushwagg,
        CreatureType::Caitian => SubType::Caitian,
        CreatureType::Camarid => SubType::Camarid,
        CreatureType::Camel => SubType::Camel,
        CreatureType::Capybara => SubType::Capybara,
        CreatureType::Caribou => SubType::Caribou,
        CreatureType::Carrier => SubType::Carrier,
        CreatureType::Cat => SubType::Cat,
        CreatureType::Centaur => SubType::Centaur,
        CreatureType::Chimera => SubType::Chimera,
        CreatureType::Citizen => SubType::Citizen,
        CreatureType::Cleric => SubType::Cleric,
        CreatureType::Clown => SubType::Clown,
        CreatureType::Cockatrice => SubType::Cockatrice,
        CreatureType::Construct => SubType::Construct,
        CreatureType::Coward => SubType::Coward,
        CreatureType::Coyote => SubType::Coyote,
        CreatureType::Crab => SubType::Crab,
        CreatureType::Crocodile => SubType::Crocodile,
        CreatureType::Ctan => SubType::Ctan,
        CreatureType::Custodes => SubType::Custodes,
        CreatureType::Cyberman => SubType::Cyberman,
        CreatureType::Cyclops => SubType::Cyclops,
        CreatureType::Dalek => SubType::Dalek,
        CreatureType::Dauthi => SubType::Dauthi,
        CreatureType::Demigod => SubType::Demigod,
        CreatureType::Demon => SubType::Demon,
        CreatureType::Deserter => SubType::Deserter,
        CreatureType::Detective => SubType::Detective,
        CreatureType::Devil => SubType::Devil,
        CreatureType::Dinosaur => SubType::Dinosaur,
        CreatureType::Djinn => SubType::Djinn,
        CreatureType::Doctor => SubType::Doctor,
        CreatureType::Dog => SubType::Dog,
        CreatureType::Dragon => SubType::Dragon,
        CreatureType::Drake => SubType::Drake,
        CreatureType::Dreadnought => SubType::Dreadnought,
        CreatureType::Drix => SubType::Drix,
        CreatureType::Drone => SubType::Drone,
        CreatureType::Druid => SubType::Druid,
        CreatureType::Dryad => SubType::Dryad,
        CreatureType::Dwarf => SubType::Dwarf,
        CreatureType::Echidna => SubType::Echidna,
        CreatureType::Efreet => SubType::Efreet,
        CreatureType::Egg => SubType::Egg,
        CreatureType::Elder => SubType::Elder,
        CreatureType::Eldrazi => SubType::Eldrazi,
        CreatureType::Elemental => SubType::Elemental,
        CreatureType::Elephant => SubType::Elephant,
        CreatureType::Elf => SubType::Elf,
        CreatureType::Elk => SubType::Elk,
        CreatureType::Employee => SubType::Employee,
        CreatureType::Eternal => SubType::Eternal,
        CreatureType::Eye => SubType::Eye,
        CreatureType::Faerie => SubType::Faerie,
        CreatureType::Ferret => SubType::Ferret,
        CreatureType::Fish => SubType::Fish,
        CreatureType::Flagbearer => SubType::Flagbearer,
        CreatureType::Fox => SubType::Fox,
        CreatureType::Fractal => SubType::Fractal,
        CreatureType::Frog => SubType::Frog,
        CreatureType::Fungus => SubType::Fungus,
        CreatureType::Gamer => SubType::Gamer,
        CreatureType::Gamma => SubType::Gamma,
        CreatureType::Gargoyle => SubType::Gargoyle,
        CreatureType::Germ => SubType::Germ,
        CreatureType::Giant => SubType::Giant,
        CreatureType::Giraffe => SubType::Giraffe,
        CreatureType::Gith => SubType::Gith,
        CreatureType::Glimmer => SubType::Glimmer,
        CreatureType::Gnoll => SubType::Gnoll,
        CreatureType::Gnome => SubType::Gnome,
        _ => unreachable!("validated dispatch"),
    }
}

fn creature_type_case_1(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Goat => SubType::Goat,
        CreatureType::Goblin => SubType::Goblin,
        CreatureType::God => SubType::God,
        CreatureType::Golem => SubType::Golem,
        CreatureType::Gorgon => SubType::Gorgon,
        CreatureType::Gorn => SubType::Gorn,
        CreatureType::Graveborn => SubType::Graveborn,
        CreatureType::Gremlin => SubType::Gremlin,
        CreatureType::Griffin => SubType::Griffin,
        CreatureType::Guest => SubType::Guest,
        CreatureType::Hag => SubType::Hag,
        CreatureType::Halfling => SubType::Halfling,
        CreatureType::Hamster => SubType::Hamster,
        CreatureType::Harpy => SubType::Harpy,
        CreatureType::Hedgehog => SubType::Hedgehog,
        CreatureType::Hellion => SubType::Hellion,
        CreatureType::Hero => SubType::Hero,
        CreatureType::Hippo => SubType::Hippo,
        CreatureType::Hippogriff => SubType::Hippogriff,
        CreatureType::Homarid => SubType::Homarid,
        CreatureType::Homunculus => SubType::Homunculus,
        CreatureType::Horror => SubType::Horror,
        CreatureType::Horse => SubType::Horse,
        CreatureType::Human => SubType::Human,
        CreatureType::Hydra => SubType::Hydra,
        CreatureType::Hyena => SubType::Hyena,
        CreatureType::Illusion => SubType::Illusion,
        CreatureType::Imp => SubType::Imp,
        CreatureType::Incarnation => SubType::Incarnation,
        CreatureType::Inhuman => SubType::Inhuman,
        CreatureType::Inkling => SubType::Inkling,
        CreatureType::Inquisitor => SubType::Inquisitor,
        CreatureType::Insect => SubType::Insect,
        CreatureType::Jackal => SubType::Jackal,
        CreatureType::Jellyfish => SubType::Jellyfish,
        CreatureType::Juggernaut => SubType::Juggernaut,
        CreatureType::Kangaroo => SubType::Kangaroo,
        CreatureType::Kavu => SubType::Kavu,
        CreatureType::Kelpien => SubType::Kelpien,
        CreatureType::Kirin => SubType::Kirin,
        CreatureType::Kithkin => SubType::Kithkin,
        CreatureType::Klingon => SubType::Klingon,
        CreatureType::Knight => SubType::Knight,
        CreatureType::Kobold => SubType::Kobold,
        CreatureType::Kor => SubType::Kor,
        CreatureType::Kraken => SubType::Kraken,
        CreatureType::Kree => SubType::Kree,
        CreatureType::Lamia => SubType::Lamia,
        CreatureType::Lammasu => SubType::Lammasu,
        CreatureType::Lanthanite => SubType::Lanthanite,
        CreatureType::Leech => SubType::Leech,
        CreatureType::Lemur => SubType::Lemur,
        CreatureType::Leviathan => SubType::Leviathan,
        CreatureType::Lhurgoyf => SubType::Lhurgoyf,
        CreatureType::Licid => SubType::Licid,
        CreatureType::Lizard => SubType::Lizard,
        CreatureType::Llama => SubType::Llama,
        CreatureType::Lobster => SubType::Lobster,
        CreatureType::Manticore => SubType::Manticore,
        CreatureType::Masticore => SubType::Masticore,
        CreatureType::Mercenary => SubType::Mercenary,
        CreatureType::Merfolk => SubType::Merfolk,
        CreatureType::Metathran => SubType::Metathran,
        CreatureType::Minion => SubType::Minion,
        CreatureType::Minotaur => SubType::Minotaur,
        CreatureType::Mite => SubType::Mite,
        CreatureType::Mole => SubType::Mole,
        CreatureType::Monger => SubType::Monger,
        CreatureType::Mongoose => SubType::Mongoose,
        CreatureType::Monk => SubType::Monk,
        CreatureType::Monkey => SubType::Monkey,
        CreatureType::Moogle => SubType::Moogle,
        CreatureType::Moonfolk => SubType::Moonfolk,
        CreatureType::Mount => SubType::Mount,
        CreatureType::Mouse => SubType::Mouse,
        CreatureType::Mutant => SubType::Mutant,
        CreatureType::Myr => SubType::Myr,
        CreatureType::Mystic => SubType::Mystic,
        CreatureType::Nautilus => SubType::Nautilus,
        CreatureType::Necron => SubType::Necron,
        CreatureType::Nephilim => SubType::Nephilim,
        CreatureType::Nightmare => SubType::Nightmare,
        CreatureType::Nightstalker => SubType::Nightstalker,
        CreatureType::Ninja => SubType::Ninja,
        CreatureType::Noble => SubType::Noble,
        CreatureType::Noggle => SubType::Noggle,
        CreatureType::Nomad => SubType::Nomad,
        CreatureType::Nymph => SubType::Nymph,
        CreatureType::Octopus => SubType::Octopus,
        CreatureType::Officer => SubType::Officer,
        CreatureType::Ogre => SubType::Ogre,
        CreatureType::Ooze => SubType::Ooze,
        CreatureType::Orb => SubType::Orb,
        CreatureType::Orc => SubType::Orc,
        CreatureType::Orgg => SubType::Orgg,
        CreatureType::Orion => SubType::Orion,
        CreatureType::Otter => SubType::Otter,
        CreatureType::Ouphe => SubType::Ouphe,
        CreatureType::Ox => SubType::Ox,
        CreatureType::Oyster => SubType::Oyster,
        CreatureType::Pangolin => SubType::Pangolin,
        CreatureType::Peasant => SubType::Peasant,
        CreatureType::Pegasus => SubType::Pegasus,
        CreatureType::Pentavite => SubType::Pentavite,
        CreatureType::Performer => SubType::Performer,
        CreatureType::Pest => SubType::Pest,
        CreatureType::Phelddagrif => SubType::Phelddagrif,
        CreatureType::Phoenix => SubType::Phoenix,
        CreatureType::Phyrexian => SubType::Phyrexian,
        CreatureType::Pilot => SubType::Pilot,
        _ => unreachable!("validated dispatch"),
    }
}

fn creature_type_case_2(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Pincher => SubType::Pincher,
        CreatureType::Pirate => SubType::Pirate,
        CreatureType::Plant => SubType::Plant,
        CreatureType::Platypus => SubType::Platypus,
        CreatureType::Porcupine => SubType::Porcupine,
        CreatureType::Possum => SubType::Possum,
        CreatureType::Praetor => SubType::Praetor,
        CreatureType::Primarch => SubType::Primarch,
        CreatureType::Prism => SubType::Prism,
        CreatureType::Processor => SubType::Processor,
        CreatureType::Q => SubType::Q,
        CreatureType::Qu => SubType::Qu,
        CreatureType::Rabbit => SubType::Rabbit,
        CreatureType::Raccoon => SubType::Raccoon,
        CreatureType::Ranger => SubType::Ranger,
        CreatureType::Rat => SubType::Rat,
        CreatureType::Rebel => SubType::Rebel,
        CreatureType::Reflection => SubType::Reflection,
        CreatureType::Rhino => SubType::Rhino,
        CreatureType::Rigger => SubType::Rigger,
        CreatureType::Robot => SubType::Robot,
        CreatureType::Rogue => SubType::Rogue,
        CreatureType::Romulan => SubType::Romulan,
        CreatureType::Sable => SubType::Sable,
        CreatureType::Salamander => SubType::Salamander,
        CreatureType::Samurai => SubType::Samurai,
        CreatureType::Sand => SubType::Sand,
        CreatureType::Saproling => SubType::Saproling,
        CreatureType::Satyr => SubType::Satyr,
        CreatureType::Scarecrow => SubType::Scarecrow,
        CreatureType::Scientist => SubType::Scientist,
        CreatureType::Scion => SubType::Scion,
        CreatureType::Scorpion => SubType::Scorpion,
        CreatureType::Scout => SubType::Scout,
        CreatureType::Sculpture => SubType::Sculpture,
        CreatureType::Seal => SubType::Seal,
        CreatureType::Serf => SubType::Serf,
        CreatureType::Serpent => SubType::Serpent,
        CreatureType::Servo => SubType::Servo,
        CreatureType::Shade => SubType::Shade,
        CreatureType::Shaman => SubType::Shaman,
        CreatureType::Shapeshifter => SubType::Shapeshifter,
        CreatureType::Shark => SubType::Shark,
        CreatureType::Sheep => SubType::Sheep,
        CreatureType::Shiar => SubType::Shiar,
        CreatureType::Siren => SubType::Siren,
        CreatureType::Skeleton => SubType::Skeleton,
        CreatureType::Skrull => SubType::Skrull,
        CreatureType::Skunk => SubType::Skunk,
        CreatureType::Slith => SubType::Slith,
        CreatureType::Sliver => SubType::Sliver,
        CreatureType::Sloth => SubType::Sloth,
        CreatureType::Slug => SubType::Slug,
        CreatureType::Snail => SubType::Snail,
        CreatureType::Snake => SubType::Snake,
        CreatureType::Soldier => SubType::Soldier,
        CreatureType::Soltari => SubType::Soltari,
        CreatureType::Sorcerer => SubType::Sorcerer,
        CreatureType::Spawn => SubType::Spawn,
        CreatureType::Specter => SubType::Specter,
        CreatureType::Spellshaper => SubType::Spellshaper,
        CreatureType::Sphinx => SubType::Sphinx,
        CreatureType::Spider => SubType::Spider,
        CreatureType::Spike => SubType::Spike,
        CreatureType::Spirit => SubType::Spirit,
        CreatureType::Splinter => SubType::Splinter,
        CreatureType::Sponge => SubType::Sponge,
        CreatureType::Spy => SubType::Spy,
        CreatureType::Squid => SubType::Squid,
        CreatureType::Squirrel => SubType::Squirrel,
        CreatureType::Starfish => SubType::Starfish,
        CreatureType::Surrakar => SubType::Surrakar,
        CreatureType::Survivor => SubType::Survivor,
        CreatureType::Symbiote => SubType::Symbiote,
        CreatureType::Synth => SubType::Synth,
        CreatureType::Talosian => SubType::Talosian,
        CreatureType::Tellarite => SubType::Tellarite,
        CreatureType::Tentacle => SubType::Tentacle,
        CreatureType::Tetravite => SubType::Tetravite,
        CreatureType::Thalakos => SubType::Thalakos,
        CreatureType::Tholian => SubType::Tholian,
        CreatureType::Thopter => SubType::Thopter,
        CreatureType::Thrull => SubType::Thrull,
        CreatureType::Tiefling => SubType::Tiefling,
        CreatureType::TimeLord => SubType::TimeLord,
        CreatureType::Tosk => SubType::Tosk,
        CreatureType::Toy => SubType::Toy,
        CreatureType::Treefolk => SubType::Treefolk,
        CreatureType::Trilobite => SubType::Trilobite,
        CreatureType::Triskelavite => SubType::Triskelavite,
        CreatureType::Troll => SubType::Troll,
        CreatureType::Turtle => SubType::Turtle,
        CreatureType::Tyranid => SubType::Tyranid,
        CreatureType::Unicorn => SubType::Unicorn,
        CreatureType::Utrom => SubType::Utrom,
        CreatureType::Vampire => SubType::Vampire,
        CreatureType::Varmint => SubType::Varmint,
        CreatureType::Vedalken => SubType::Vedalken,
        CreatureType::Villain => SubType::Villain,
        CreatureType::Volver => SubType::Volver,
        CreatureType::Vorta => SubType::Vorta,
        CreatureType::Vulcan => SubType::Vulcan,
        CreatureType::Wall => SubType::Wall,
        CreatureType::Walrus => SubType::Walrus,
        CreatureType::Warlock => SubType::Warlock,
        CreatureType::Warrior => SubType::Warrior,
        CreatureType::Weasel => SubType::Weasel,
        CreatureType::Weird => SubType::Weird,
        CreatureType::Werewolf => SubType::Werewolf,
        CreatureType::Whale => SubType::Whale,
        _ => unreachable!("validated dispatch"),
    }
}

fn creature_type_case_3(value: &CreatureType) -> SubType {
    match value {
        CreatureType::Wizard => SubType::Wizard,
        CreatureType::Wolf => SubType::Wolf,
        CreatureType::Wolverine => SubType::Wolverine,
        CreatureType::Wombat => SubType::Wombat,
        CreatureType::Worm => SubType::Worm,
        CreatureType::Wraith => SubType::Wraith,
        CreatureType::Wurm => SubType::Wurm,
        CreatureType::Xindi => SubType::Xindi,
        CreatureType::Yeti => SubType::Yeti,
        CreatureType::Zombie => SubType::Zombie,
        CreatureType::Zubera => SubType::Zubera,
        _ => unreachable!("validated dispatch"),
    }
}

pub fn land_type(value: &LandType) -> SubType {
    match value {
        LandType::Cave => SubType::Cave,
        LandType::Desert => SubType::Desert,
        LandType::Forest => SubType::Forest,
        LandType::Gate => SubType::Gate,
        LandType::Island => SubType::Island,
        LandType::Lair => SubType::Lair,
        LandType::Locus => SubType::Locus,
        LandType::Mine => SubType::Mine,
        LandType::Mountain => SubType::Mountain,
        LandType::Plains => SubType::Plains,
        LandType::Planet => SubType::Planet,
        LandType::PowerPlant => SubType::PowerPlant,
        LandType::Sphere => SubType::Sphere,
        LandType::Swamp => SubType::Swamp,
        LandType::Tower => SubType::Tower,
        LandType::Town => SubType::Town,
        LandType::Urzas => SubType::Urzas,
    }
}

pub fn artifact_type(value: &ArtifactType) -> SubType {
    match value {
        ArtifactType::Attraction => SubType::Attraction,
        ArtifactType::Blood => SubType::Blood,
        ArtifactType::Bobblehead => SubType::Bobblehead,
        ArtifactType::Book => SubType::Book,
        ArtifactType::Clue => SubType::Clue,
        ArtifactType::Equipment => SubType::Equipment,
        ArtifactType::Food => SubType::Food,
        ArtifactType::Fortification => SubType::Fortification,
        ArtifactType::Gold => SubType::Gold,
        ArtifactType::Heartwood => SubType::Heartwood,
        ArtifactType::Incubator => SubType::Incubator,
        ArtifactType::Infinity => SubType::Infinity,
        ArtifactType::Junk => SubType::Junk,
        ArtifactType::Lander => SubType::Lander,
        ArtifactType::Map => SubType::Map,
        ArtifactType::Mutagen => SubType::Mutagen,
        ArtifactType::Powerstone => SubType::Powerstone,
        ArtifactType::Spacecraft => SubType::Spacecraft,
        ArtifactType::Stone => SubType::Stone,
        ArtifactType::Treasure => SubType::Treasure,
        ArtifactType::Vehicle => SubType::Vehicle,
        ArtifactType::Vibranium => SubType::Vibranium,
    }
}

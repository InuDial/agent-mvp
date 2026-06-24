pub mod fs;
pub mod network;

// Everything that outlives one session should be a Job

// Everything that calls AI inside app (i.e. except through network, etc)
// should be an agent.

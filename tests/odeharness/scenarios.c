/*
** SPDX-FileCopyrightText: 2026 Maksym Motorny
** SPDX-License-Identifier: BSD-3-Clause
**
** The comparison's scenarios (harness.h): synthetic scenes of our own, no
** game data, driven through the library's public API in a fixed order, and
** what the library computes recorded after every step. Each starts with the
** shipped library's floating-point environment (24-bit precision, round to
** nearest, exceptions masked; MXCSR flush-to-zero and denormals-are-zero)
** and the run time's rand at 1, except the *-nodaz ones (MXCSR's default),
** which tell whether denormals change the shipped library's results (#769).
**
** Inputs are made from integers and bit patterns; the only float arithmetic
** here is exact or single precision on both sides (SSE on Windows).
*/

#include "harness.h"

#include <string.h>

#define NONE ((void *) 0)

static const OdeHarness *H;

/* ---- output ---- */

enum {
  T_PARAMS = 1, T_X87, T_VEC, T_BODY, T_FEEDBACK, T_JOINT, T_CONTACTS,
  T_COUNT, T_GEOM, T_FLAG, T_MASS, T_CONTACT, T_ROT
};

static uint32_t bits (float f)
{
  uint32_t u;
  memcpy (&u, &f, 4);
  return u;
}

static float from_bits (uint32_t u)
{
  float f;
  memcpy (&f, &u, 4);
  return f;
}

static void rec (uint32_t tag, const uint32_t *w, uint32_t n)
{
  H->record (H->ctx, tag, w, n);
}

static void rec_floats (uint32_t tag, const dReal *v, uint32_t n)
{
  uint32_t w[64];
  for (uint32_t i = 0; i < n; i++) w[i] = bits (v[i]);
  rec (tag, w, n);
}

static void rec_int (uint32_t tag, uint32_t v)
{
  rec (tag, &v, 1);
}

static void rec_body (dBodyID b)
{
  dReal v[25];
  memcpy (v, H->dBodyGetPosition (b), 3 * 4);
  memcpy (v + 3, H->dBodyGetQuaternion (b), 4 * 4);
  memcpy (v + 7, H->dBodyGetRotation (b), 12 * 4);
  memcpy (v + 19, H->dBodyGetLinearVel (b), 3 * 4);
  memcpy (v + 22, H->dBodyGetAngularVel (b), 3 * 4);
  rec_floats (T_BODY, v, 25);
}

static void rec_params (dWorldID w)
{
  dReal v[12];
  H->dWorldGetGravity (w, v);
  v[3] = H->dWorldGetERP (w);
  v[4] = H->dWorldGetCFM (w);
  v[5] = from_bits ((uint32_t) H->dWorldGetQuickStepNumIterations (w));
  v[6] = H->dWorldGetQuickStepW (w);
  v[7] = H->dWorldGetContactSurfaceLayer (w);
  v[8] = H->dWorldGetContactMaxCorrectingVel (w);
  v[9] = H->dWorldGetAutoDisableLinearThreshold (w);
  v[10] = from_bits ((uint32_t) H->dWorldGetAutoDisableSteps (w));
  v[11] = from_bits ((uint32_t) H->dWorldGetAutoDisableFlag (w));
  rec_floats (T_PARAMS, v, 12);
}

/* ---- inputs: integers and bit patterns ---- */

static uint32_t seed;

static uint32_t next (void)
{
  seed = seed * 1664525u + 1013904223u;
  return seed >> 8;
}

/* a float in [lo, hi): an integer of 24 bits scaled exactly, then one
** single-precision multiply-add (the same on both sides) */
static float uni (float lo, float hi)
{
  float u = (float) next () * (1.0f / 16777216.0f);
  return lo + (hi - lo) * u;
}

/* a float with random sign and mantissa, its exponent in [emin, emax] */
static float spread (int emin, int emax)
{
  uint32_t e = (uint32_t) (emin + 127) + next () % (uint32_t) (emax - emin + 1);
  return from_bits ((next () & 1u) << 31 | e << 23 | (next () & 0x7FFFFFu));
}

static void rand_rotation (dReal *R)
{
  float ax = uni (-1, 1), ay = uni (-1, 1), az = uni (-1, 1);
  H->dRFromAxisAndAngle (R, ax, ay, az + 0.01f, uni (-3.1f, 3.1f));
}

/* ---- the game's step and parameters (physics-world.md 1.2, 1.3) ---- */

static const float STEP = 0.02f;

static dWorldID game_world (void)
{
  dWorldID w = H->dWorldCreate ();
  H->dWorldSetGravity (w, 0, -19.62f, 0);
  H->dWorldSetERP (w, 0.54545456f);	/* 6/11 */
  H->dWorldSetCFM (w, 1.1363636e-6f);	/* 1/880,000 */
  return w;
}

static void mass_box (dBodyID b, float total, float lx, float ly, float lz)
{
  dMass m;
  H->dMassSetBoxTotal (&m, total, lx, ly, lz);
  H->dBodySetMass (b, &m);
}

/* ---- scenarios ---- */

/* the library's values when it loads, then after dWorldCreate */
static void s_library (void)
{
  /* a stand-in world: the getters read the library-wide values */
  static uint32_t stand_in[16];
  rec_params ((dWorldID) (void *) stand_in);
  dWorldID w = H->dWorldCreate ();
  rec_params (w);
  H->dWorldDestroy (w);
}

/* the x87 instructions themselves, against our emulation */
static void s_x87 (void)
{
  static const uint32_t special[] = {
    0x00000000, 0x80000000, 0x00000001, 0x007FFFFF, 0x00800000, 0x3F800000,
    0x40490FDB, 0xC0490FDB, 0x3FC90FDB, 0x3F490FDB, 0x40C90FDB, 0x41490FDB,
    0x4B800000, 0x5F000000, 0x5EFFFFFF, 0x7F7FFFFF, 0x7F800000, 0xFF800000,
    0x7FC00000, 0x2F800000, 0x38D1B717, 0x3EAAAAAB, 0x447A0000, 0x49742400
  };
  int n = (int) (sizeof special / sizeof special[0]);
  for (int op = 0; op < 3; op++) {
    uint32_t w[2];
    for (int i = 0; i < n; i++) {
      w[0] = special[i];
      w[1] = bits (H->x87 (op, 0, from_bits (special[i])));
      rec (T_X87 + (uint32_t) (op << 8), w, 2);
    }
    for (int i = 0; i < 4000; i++) {
      float x = op == 0 ? from_bits (bits (spread (-140, 127)) & 0x7FFFFFFF)
		: (i & 1) ? spread (-12, 4) : uni (-30.0f, 30.0f);
      w[0] = bits (x);
      w[1] = bits (H->x87 (op, 0, x));
      rec (T_X87 + (uint32_t) (op << 8), w, 2);
    }
  }
  for (int i = 0; i < n * n; i += 7) {
    uint32_t w[3] = {special[i / n], special[i % n], 0};
    w[2] = bits (H->x87 (3, from_bits (w[0]), from_bits (w[1])));
    rec (T_X87 + (3 << 8), w, 3);
  }
  for (int i = 0; i < 4000; i++) {
    float y = (i & 1) ? spread (-30, 30) : uni (-5, 5);
    float x = (i & 2) ? spread (-30, 30) : uni (-5, 5);
    uint32_t w[3] = {bits (y), bits (x), bits (H->x87 (3, y, x))};
    rec (T_X87 + (3 << 8), w, 3);
  }
}

static void s_normalise (void)
{
  for (int i = 0; i < 2000; i++) {
    int e = (int) (next () % 40) - 30;
    dReal v[4] = {spread (e - 3, e), spread (e - 3, e), spread (e - 3, e), 0};
    if (i % 50 == 0) v[0] = v[1] = v[2] = 0;
    dReal in[3] = {v[0], v[1], v[2]};
    H->dNormalize3_slow (v);
    rec_floats (T_VEC, in, 3);
    rec_floats (T_VEC, v, 3);
  }
}

/* bodies without joints or contacts: forces, torques, the gyroscopic term,
** finite rotations, no gravity; both steppers */
static void s_free (void)
{
  dWorldID w = game_world ();
  dBodyID b[8];
  for (int i = 0; i < 8; i++) {
    b[i] = H->dBodyCreate (w);
    mass_box (b[i], uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    H->dBodySetPosition (b[i], uni (-5, 5), uni (0, 10), uni (-5, 5));
    dReal R[12];
    rand_rotation (R);
    H->dBodySetRotation (b[i], R);
    H->dBodySetLinearVel (b[i], uni (-3, 3), uni (-3, 3), uni (-3, 3));
    H->dBodySetAngularVel (b[i], uni (-5, 5), uni (-5, 5), uni (-5, 5));
  }
  H->dBodySetFiniteRotationMode (b[1], 1);
  H->dBodySetFiniteRotationMode (b[2], 1);
  H->dBodySetFiniteRotationAxis (b[2], 0.3f, 1, 0.2f);
  H->dBodySetGravityMode (b[3], 0);
  for (int s = 0; s < 200; s++) {
    for (int i = 0; i < 8; i++) {
      H->dBodyAddForce (b[i], uni (-50, 50), uni (-50, 50), uni (-50, 50));
      H->dBodyAddTorque (b[i], uni (-1, 1), uni (-1, 1), uni (-1, 1));
      H->dBodyAddRelForceAtRelPos (b[i], uni (-9, 9), 0, uni (-9, 9), 0.1f, 0.2f, uni (-1, 1));
    }
    if (s < 100) H->dWorldQuickStep (w, STEP);
    else H->dWorldStep (w, STEP);
    for (int i = 0; i < 8; i++) rec_body (b[i]);
  }
  H->dWorldDestroy (w);
}

/* every joint kind, with limits, motors and feedback; both steppers */
static void s_joints (void)
{
  dWorldID w = game_world ();
  enum { N = 9 };
  dBodyID b[N];
  for (int i = 0; i < N; i++) {
    b[i] = H->dBodyCreate (w);
    mass_box (b[i], uni (2, 40), 0.3f, 0.3f, 0.6f);
    H->dBodySetPosition (b[i], (float) i * 0.5f, 5, 0);
    H->dBodySetAngularVel (b[i], uni (-2, 2), uni (-2, 2), uni (-2, 2));
  }
  dJointID j[N];
  static dJointFeedback fb[N];
  j[0] = H->dJointCreateBall (w, 0);
  H->dJointAttach (j[0], b[0], NONE);		/* hung from the world */
  H->dJointSetBallAnchor (j[0], -0.25f, 5, 0);
  j[1] = H->dJointCreateHinge (w, 0);
  H->dJointAttach (j[1], b[0], b[1]);
  H->dJointSetHingeAnchor (j[1], 0.25f, 5, 0);
  H->dJointSetHingeAxis (j[1], 0, 0, 1);
  H->dJointSetHingeParam (j[1], dParamLoStop, -0.5f);
  H->dJointSetHingeParam (j[1], dParamHiStop, 0.7f);
  H->dJointSetHingeParam (j[1], dParamBounce, 0.3f);
  j[2] = H->dJointCreateHinge (0, 0);		/* made without a world */
  H->dWorldAddJoint (w, j[2]);
  H->dJointAttach (j[2], b[1], b[2]);
  H->dJointSetHingeAnchor (j[2], 0.75f, 5, 0);
  H->dJointSetHingeAxis (j[2], 0, 1, 0);
  H->dJointSetHingeParam (j[2], dParamVel, 2);
  H->dJointSetHingeParam (j[2], dParamFMax, 30);
  j[3] = H->dJointCreateSlider (w, 0);
  H->dJointAttach (j[3], b[2], b[3]);
  H->dJointSetSliderAxis (j[3], 1, 0.2f, 0);
  H->dJointSetSliderParam (j[3], dParamLoStop, -0.1f);
  H->dJointSetSliderParam (j[3], dParamHiStop, 0.2f);
  j[4] = H->dJointCreateUniversal (w, 0);
  H->dJointAttach (j[4], b[3], b[4]);
  H->dJointSetUniversalAnchor (j[4], 1.75f, 5, 0);
  H->dJointSetUniversalAxis1 (j[4], 0, 1, 0);
  H->dJointSetUniversalAxis2 (j[4], 0, 0, 1);
  j[5] = H->dJointCreateHinge2 (w, 0);
  H->dJointAttach (j[5], b[4], b[5]);
  H->dJointSetHinge2Anchor (j[5], 2.25f, 5, 0);
  H->dJointSetHinge2Axis1 (j[5], 0, 1, 0);
  H->dJointSetHinge2Axis2 (j[5], 1, 0, 0);
  H->dJointSetHinge2Param (j[5], dParamSuspensionERP, 0.4f);
  H->dJointSetHinge2Param (j[5], dParamSuspensionCFM, 0.01f);
  j[6] = H->dJointCreateFixed (w, 0);
  H->dJointAttach (j[6], b[5], b[6]);
  H->dJointSetFixed (j[6]);
  j[7] = H->dJointCreateAMotor (w, 0);
  H->dJointAttach (j[7], b[6], b[7]);
  H->dJointSetAMotorNumAxes (j[7], 1);
  H->dJointSetAMotorAxis (j[7], 0, 1, 0, 0, 1);
  H->dJointSetAMotorParam (j[7], dParamVel, -1);
  H->dJointSetAMotorParam (j[7], dParamFMax, 10);
  {
    /* the motor turns b7; a ball holds it */
    dJointID ball = H->dJointCreateBall (w, 0);
    H->dJointAttach (ball, b[6], b[7]);
    H->dJointSetBallAnchor (ball, 3.25f, 5, 0);
  }
  j[8] = H->dJointCreateFixed (w, 0);		/* one body held at a pose */
  H->dJointAttach (j[8], b[8], NONE);
  {
    dReal q[4] = {0.9238795f, 0, 0.38268343f, 0}, p[4] = {4, 6, 1, 0};
    H->dJointSetFixedQuaternionPos (j[8], q, p);
  }
  for (int i = 0; i < N; i++) H->dJointSetFeedback (j[i], &fb[i]);
  for (int s = 0; s < 300; s++) {
    H->dBodyAddForce (b[N / 2], uni (-20, 20), uni (-20, 20), uni (-20, 20));
    if (s < 150) H->dWorldQuickStep (w, STEP);
    else H->dWorldStep (w, STEP);
    for (int i = 0; i < N; i++) rec_body (b[i]);
    for (int i = 0; i < N; i++) rec_floats (T_FEEDBACK, (const dReal *) &fb[i], 16);
    dReal a[4] = {H->dJointGetHingeAngle (j[1]), H->dJointGetHingeAngleRate (j[1]),
		  H->dJointGetHingeAngle (j[2]), H->dJointGetSliderPosition (j[3])};
    rec_floats (T_JOINT, a, 4);
  }
  H->dWorldDestroy (w);
}

/* a pile on a plane: contacts of both kinds through dCollide each step */
typedef struct Pile {
  dWorldID w;
  dSpaceID space;
  dJointGroupID group;
  dJointID made[512];
  int nmade, step_contacts;
} Pile;

static void near_cb (void *data, dGeomID a, dGeomID b)
{
  Pile *p = (Pile *) data;
  dBodyID ba = H->dGeomGetBody (a), bb = H->dGeomGetBody (b);
  if (ba && bb && H->dAreConnected (ba, bb)) return;
  dContactGeom cg[16];
  int n = H->dCollide (a, b, 8, cg, (int) sizeof (dContactGeom));
  uint32_t w[1 + 8 * 7];
  w[0] = (uint32_t) n;
  for (int i = 0; i < n && i < 8; i++) {
    for (int k = 0; k < 3; k++) w[1 + i * 7 + k] = bits (cg[i].pos[k]);
    for (int k = 0; k < 3; k++) w[4 + i * 7 + k] = bits (cg[i].normal[k]);
    w[7 + i * 7] = bits (cg[i].depth);
  }
  rec (T_CONTACTS, w, 1 + (uint32_t) (n < 8 ? n : 8) * 7);
  for (int i = 0; i < n && p->nmade < 512; i++) {
    dContact c;
    memset (&c, 0, sizeof c);
    int kind = (p->step_contacts + i) % 4;
    c.surface.mode = dContactApprox1 | dContactSoftERP | dContactSoftCFM;
    c.surface.mu = kind == 3 ? 0.3f : 1.0f;
    c.surface.soft_erp = 0.5f;
    c.surface.soft_cfm = 1e-5f;
    if (kind == 1) {
      c.surface.mode |= dContactBounce;
      c.surface.bounce = 0.4f;
      c.surface.bounce_vel = 0.1f;
    }
    if (kind == 2) {
      c.surface.mode |= dContactMu2;
      c.surface.mu2 = 0.5f;
    }
    c.geom = cg[i];
    dJointID j = (i & 1) ? H->dJointCreateContactSpecial (0, p->group, &c)
			 : H->dJointCreateContact (0, p->group, &c);
    H->dWorldAddJoint (p->w, j);
    H->dJointAttach (j, ba, bb);
    p->made[p->nmade++] = j;
  }
  p->step_contacts += n;
}

static void pile_step (Pile *p, int exact)
{
  p->nmade = 0;
  p->step_contacts = 0;
  H->dSpaceCollide (p->space, p, near_cb);
  rec_int (T_COUNT, (uint32_t) p->nmade);
  if (exact) H->dWorldStep (p->w, STEP);
  else H->dWorldQuickStep (p->w, STEP);
  /* the contacts detached and out of the world, then the group emptied
  ** (emptying detaches only joints still in a world, and leaves them
  ** listed: physics-library.md 5.2) */
  for (int i = 0; i < p->nmade; i++) {
    H->dJointAttach (p->made[i], NONE, NONE);
    H->dWorldRemoveJoint (p->w, p->made[i]);
  }
  H->dJointGroupEmpty (p->group);
}

static void pile (int count, int steps, int exact, float heavy)
{
  Pile p;
  p.w = game_world ();
  p.space = H->dSimpleSpaceCreate (0);
  p.group = H->dJointGroupCreate (0);
  H->dCreatePlane (p.space, 0, 1, 0, 0);
  dBodyID b[32];
  for (int i = 0; i < count; i++) {
    b[i] = H->dBodyCreate (p.w);
    dGeomID g;
    float s1 = uni (0.2f, 1), s2 = uni (0.2f, 1), s3 = uni (0.2f, 1);
    float m = heavy > 0 ? heavy : uni (5, 100);
    dMass ms;
    switch (i % 3) {
    case 0:
      H->dMassSetBoxTotal (&ms, m, s1, s2, s3);
      g = H->dCreateBox (p.space, s1, s2, s3);
      break;
    case 1:
      H->dMassSetSphereTotal (&ms, m, s1 * 0.5f);
      g = H->dCreateSphere (p.space, s1 * 0.5f);
      break;
    default:
      H->dMassSetCappedCylinderTotal (&ms, m, 3, s1 * 0.3f, s2);
      g = H->dCreateCCylinder (p.space, s1 * 0.3f, s2);
      break;
    }
    H->dBodySetMass (b[i], &ms);
    H->dGeomSetBody (g, b[i]);
    H->dBodySetPosition (b[i], uni (-1, 1), 0.6f + (float) i * 0.8f, uni (-1, 1));
    dReal R[12];
    rand_rotation (R);
    H->dBodySetRotation (b[i], R);
  }
  for (int s = 0; s < steps; s++) {
    pile_step (&p, exact);
    for (int i = 0; i < count; i++) rec_body (b[i]);
  }
  H->dJointGroupDestroy (p.group);
  H->dSpaceDestroy (p.space);
  H->dWorldDestroy (p.w);
}

static void s_contacts (void) { pile (12, 400, 0, 0); }
static void s_contacts_exact (void) { pile (6, 200, 1, 0); }
static void s_heavy (void) { pile (6, 150, 0, 20000); }

/* geometry pairs through dCollide, at most 1 and at most 8 asked for */
static void s_pairs (void)
{
  dGeomID box1 = H->dCreateBox (0, 1, 1, 1), box2 = H->dCreateBox (0, 1, 1, 1);
  dGeomID sphere = H->dCreateSphere (0, 0.5f), ccyl = H->dCreateCCylinder (0, 0.3f, 1);
  dGeomID plane = H->dCreatePlane (0, 0, 1, 0, 0.1f);
  dGeomID first[5] = {box1, sphere, ccyl, ccyl, box1};
  dGeomID second[5] = {box2, box2, box2, ccyl, plane};
  for (int k = 0; k < 5; k++) {
    for (int i = 0; i < 1500; i++) {
      dReal R[12];
      H->dGeomSetPosition (first[k], 0, 0, 0);
      rand_rotation (R);
      H->dGeomSetRotation (first[k], R);
      if (second[k] != plane && second[k] != first[k]) {
	rand_rotation (R);
	H->dGeomSetRotation (second[k], R);
	H->dGeomSetPosition (second[k], uni (-1.2f, 1.2f), uni (-1.2f, 1.2f), uni (-1.2f, 1.2f));
      }
      if (k == 3) {
	/* two capped cylinders: a second one */
	static dGeomID other;
	if (!other) other = H->dCreateCCylinder (0, 0.25f, 0.8f);
	rand_rotation (R);
	H->dGeomSetRotation (other, R);
	H->dGeomSetPosition (other, uni (-1, 1), uni (-1, 1), uni (-1, 1));
	second[k] = other;
      }
      for (int max = 1; max <= 8; max += 7) {
	dContactGeom cg[16];
	int n = H->dCollide (first[k], second[k], max, cg, (int) sizeof (dContactGeom));
	uint32_t w[2 + 16 * 7];
	w[0] = (uint32_t) k;
	w[1] = (uint32_t) n;
	for (int c = 0; c < n && c < 16; c++) {
	  for (int e = 0; e < 3; e++) w[2 + c * 7 + e] = bits (cg[c].pos[e]);
	  for (int e = 0; e < 3; e++) w[5 + c * 7 + e] = bits (cg[c].normal[e]);
	  w[8 + c * 7] = bits (cg[c].depth);
	}
	rec (T_CONTACTS, w, 2 + (uint32_t) (n < 16 ? n : 16) * 7);
      }
    }
  }
  H->dGeomDestroy (box1);
  H->dGeomDestroy (box2);
  H->dGeomDestroy (sphere);
  H->dGeomDestroy (ccyl);
  H->dGeomDestroy (plane);
}

/* rays set from a start and direction, against every class */
static void s_rays (void)
{
  dGeomID ray = H->dCreateRay (0, 5);
  dGeomID target[3] = {H->dCreateSphere (0, 0.7f), H->dCreateBox (0, 1, 0.5f, 2),
		       H->dCreateCCylinder (0, 0.4f, 1.5f)};
  int (*direct[3]) (dGeomID, dGeomID, int, dContactGeom *, int) = {
    H->dCollideRaySphere, H->dCollideRayBox, H->dCollideRayCCylinder};
  for (int i = 0; i < 3000; i++) {
    H->dGeomRaySet (ray, uni (-3, 3), uni (-3, 3), uni (-3, 3), uni (-1, 1), uni (-1, 1), uni (-1, 1));
    dReal g[15];
    memcpy (g, H->dGeomGetPosition (ray), 12);
    memcpy (g + 3, H->dGeomGetRotation (ray), 48);
    /* the rotation's fourth column, which ODE's dGeomRaySet leaves as the
    ** stack held it (dRFromZAxis fills three columns of a local), recorded
    ** as zero */
    g[6] = g[10] = g[14] = 0;
    rec_floats (T_GEOM, g, 15);
    int t = i % 3;
    dReal R[12];
    rand_rotation (R);
    H->dGeomSetRotation (target[t], R);
    dContactGeom c[2];
    int n = (i & 1) ? H->dCollide (ray, target[t], 1, c, (int) sizeof c[0])
		    : direct[t] (ray, target[t], 1, c, (int) sizeof c[0]);
    dReal v[8] = {from_bits ((uint32_t) n), c[0].pos[0], c[0].pos[1], c[0].pos[2],
		  c[0].normal[0], c[0].normal[1], c[0].normal[2], c[0].depth};
    rec_floats (T_GEOM, v, n ? 8 : 1);
  }
  /* a ray on a body moves and turns the body */
  dWorldID w = game_world ();
  dBodyID b = H->dBodyCreate (w);
  H->dGeomSetBody (ray, b);
  H->dGeomRaySet (ray, 1, 2, 3, 0.2f, -1, 0.4f);
  rec_body (b);
  H->dGeomDestroy (ray);
  for (int t = 0; t < 3; t++) H->dGeomDestroy (target[t]);
  H->dWorldDestroy (w);
}

/* worlds as lists: bodies and joints outside a world, added and removed,
** disabled bodies, parameters shared by worlds, emptied groups, dxStepBody */
static void s_worlds (void)
{
  dWorldID a = game_world (), c = H->dWorldCreate ();
  rec_params (a);			/* c's creation reset them */
  H->dWorldSetGravity (c, 0, -19.62f, 0);
  dBodyID b[4];
  for (int i = 0; i < 4; i++) {
    b[i] = H->dBodyCreate (i < 2 ? a : NONE);
    mass_box (b[i], 10, 1, 1, 1);
    H->dBodySetPosition (b[i], (float) i * 3, 1, 0);
    H->dBodySetAngularVel (b[i], 1, 2, 3);
  }
  H->dBodyDisable (b[1]);
  H->dWorldAddBody (a, b[2]);
  dJointGroupID g = H->dJointGroupCreate (0);
  dJointID j = H->dJointCreateBall (0, g);
  H->dWorldAddJoint (a, j);
  H->dJointAttach (j, b[0], b[2]);
  H->dJointSetBallAnchor (j, 1.5f, 1, 0);
  for (int s = 0; s < 40; s++) {
    if (s == 20) {
      /* as the game: detached and out of the world, then the group
      ** emptied; the body out of the world */
      H->dJointAttach (j, NONE, NONE);
      H->dWorldRemoveJoint (a, j);
      H->dJointGroupEmpty (g);
      H->dWorldRemoveBody (a, b[2]);
    }
    H->dWorldQuickStep (a, STEP);
    for (int i = 0; i < 4; i++) rec_body (b[i]);
    rec_int (T_FLAG, (uint32_t) H->dBodyIsEnabled (b[1]));
  }
  for (int s = 0; s < 20; s++) {
    H->dxStepBody (b[3], STEP);
    rec_body (b[3]);
  }
  H->dJointGroupDestroy (g);
  H->dBodyDestroy (b[3]);
  H->dBodyDestroy (b[2]);
  H->dWorldDestroy (a);
  H->dWorldDestroy (c);
}

/* ---- the open questions of physics-harness.md ---- */

static void rec_mass (const dMass *m)
{
  rec_floats (T_MASS, &m->mass, 17);
}

/* a body alone in a world of its own, without gravity, its mass and state
** recorded, stepped once (the exact stepper when exact), recorded again */
static void step_alone (const dMass *m, const dReal *R, const dReal *lvel, const dReal *avel,
			const dReal *torque, int exact)
{
  dWorldID w = game_world ();
  dBodyID b = H->dBodyCreate (w);
  H->dBodySetMass (b, m);
  H->dBodySetGravityMode (b, 0);
  H->dBodySetPosition (b, 1, 2, 3);
  H->dBodySetRotation (b, R);
  H->dBodySetLinearVel (b, lvel[0], lvel[1], lvel[2]);
  H->dBodySetAngularVel (b, avel[0], avel[1], avel[2]);
  rec_mass (m);
  rec_body (b);
  rec_floats (T_VEC, torque, 3);
  H->dBodyAddTorque (b, torque[0], torque[1], torque[2]);
  if (exact) H->dWorldStep (w, STEP);
  else H->dWorldQuickStep (w, STEP);
  rec_body (b);
  H->dWorldDestroy (w);
}

static const dReal IDENTITY[12] = {1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0};

/* the angular update separated (4.3): a sphere's mass (a scalar inertia) or
** a box's, at the identity or a rotation, with torque or none, zero angular
** velocity (no gyroscopic term) or along one axis or all three; one step of
** each stepper from each state */
static void s_angular (void)
{
  for (int i = 0; i < 1600; i++) {
    int kind = i % 8, exact = (i / 8) & 1;
    dMass m;
    if (kind < 3) H->dMassSetSphereTotal (&m, uni (1, 200), uni (0.1f, 1));
    else H->dMassSetBoxTotal (&m, uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    dReal R[12];
    memcpy (R, IDENTITY, sizeof R);
    if (kind == 1 || kind == 4 || kind >= 6) rand_rotation (R);
    dReal lvel[3] = {0, 0, 0}, avel[3] = {0, 0, 0}, torque[3] = {0, 0, 0};
    int axis = (int) (next () % 3);
    /* kinds at a rotation: 1, 4, 6, 7; the others at the identity */
    switch (kind) {
    case 0: /* a sphere: torque, no angular velocity */
    case 1:
      for (int k = 0; k < 3; k++) torque[k] = uni (-1, 1);
      break;
    case 2: /* a sphere: angular velocity, no torque */
      for (int k = 0; k < 3; k++) avel[k] = uni (-5, 5);
      break;
    case 3: /* a box: torque about one axis, no angular velocity */
    case 4:
      torque[axis] = uni (-1, 1);
      break;
    case 5: /* a box: angular velocity about one axis, no torque */
    case 6:
      avel[axis] = uni (-5, 5);
      break;
    default: /* a box: angular velocity about all three, no torque */
      for (int k = 0; k < 3; k++) avel[k] = uni (-5, 5);
      break;
    }
    step_alone (&m, R, lvel, avel, torque, exact);
  }
}

/* dRFromAxisAndAngle's rotation (5.3), its inputs first; then the rotation
** read back from a geometry after dGeomSetRotation, and from a body after
** dBodySetRotation with its quaternion */
static void s_rotation (void)
{
  static const float special[][4] = {
    {1, 0, 0, 1}, {0, 1, 0, 1}, {0, 0, 1, 1}, {2, 0, 0, 1}, {1, 1, 1, 0},
    {1, 1, 0, 3.1415927f}, {0, 0, 1, -3.1415927f}, {1, 2, 3, 0.001f}, {0.5f, 0, 0, 1.5707964f},
    {1, 1, 1, 1}
  };
  int nspecial = (int) (sizeof special / sizeof special[0]);
  dGeomID box = H->dCreateBox (0, 1, 1, 1);
  dWorldID w = game_world ();
  dBodyID b = H->dBodyCreate (w);
  for (int i = 0; i < 3000; i++) {
    float a[4];
    if (i < nspecial) memcpy (a, special[i], sizeof a);
    else {
      /* as rand_rotation's */
      a[0] = uni (-1, 1);
      a[1] = uni (-1, 1);
      a[2] = uni (-1, 1) + 0.01f;
      a[3] = uni (-3.1f, 3.1f);
    }
    dReal R[12];
    memset (R, 0, sizeof R);
    H->dRFromAxisAndAngle (R, a[0], a[1], a[2], a[3]);
    dReal v[16];
    memcpy (v, a, sizeof a);
    memcpy (v + 4, R, sizeof R);
    rec_floats (T_ROT, v, 16);
    H->dGeomSetRotation (box, R);
    rec_floats (T_ROT, H->dGeomGetRotation (box), 12);
    H->dBodySetRotation (b, R);
    memcpy (v, H->dBodyGetQuaternion (b), 4 * 4);
    memcpy (v + 4, H->dBodyGetRotation (b), 12 * 4);
    rec_floats (T_ROT, v, 16);
  }
  H->dGeomDestroy (box);
  H->dWorldDestroy (w);
}

static const float NORMALS[][3] = {
  {0, 1, 0}, {0.6f, 0.8f, 0}, {0, 0.6f, 0.8f}, {0.48f, 0.6f, 0.64f}, {-0.36f, 0.48f, 0.8f}
};

/* one exact-stepper step (6) with its inputs recorded: two bodies, their
** masses and states, and contact joints with every input of each */
static void s_exact_step (void)
{
  for (int t = 0; t < 300; t++) {
    dWorldID w = game_world ();
    dJointGroupID group = H->dJointGroupCreate (0);
    dBodyID b[2];
    for (int i = 0; i < 2; i++) {
      b[i] = H->dBodyCreate (w);
      dMass m;
      H->dMassSetBoxTotal (&m, uni (5, 100), uni (0.2f, 1), uni (0.2f, 1), uni (0.2f, 1));
      H->dBodySetMass (b[i], &m);
      H->dBodySetPosition (b[i], uni (-0.1f, 0.1f), 0.5f + (float) i, uni (-0.1f, 0.1f));
      dReal R[12];
      rand_rotation (R);
      H->dBodySetRotation (b[i], R);
      H->dBodySetLinearVel (b[i], uni (-1, 1), uni (-1, 1), uni (-1, 1));
      H->dBodySetAngularVel (b[i], uni (-2, 2), uni (-2, 2), uni (-2, 2));
      rec_mass (&m);
      rec_body (b[i]);
    }
    /* body 0 on the ground, body 1 on body 0 and, from the third trial
    ** of four, on the ground too */
    int n0 = 1 + t % 4, n01 = 1 + (t / 4) % 2, n1 = (t % 4) >= 2;
    int n = n0 + n01 + n1;
    /* zeroed: a joint to the world leaves its second body's half alone */
    static dJointFeedback fb[8];
    memset (fb, 0, sizeof fb);
    dJointID made[8];
    for (int i = 0; i < n; i++) {
      dContact c;
      memset (&c, 0, sizeof c);
      int kind = (t + i) % 4;
      c.surface.mode = dContactApprox1 | dContactSoftERP | dContactSoftCFM;
      c.surface.mu = kind == 3 ? 0.3f : 1.0f;
      c.surface.soft_erp = 0.5f;
      c.surface.soft_cfm = 1e-5f;
      if (kind == 1) {
	c.surface.mode |= dContactBounce;
	c.surface.bounce = 0.4f;
	c.surface.bounce_vel = 0.1f;
      }
      if (kind == 2) {
	c.surface.mode |= dContactMu2;
	c.surface.mu2 = 0.5f;
      }
      dBodyID ba, bb;
      const float *normal;
      float y;
      if (i < n0) {
	ba = b[0], bb = NONE, normal = NORMALS[0], y = 0;
      } else if (i < n0 + n01) {
	ba = b[1], bb = b[0], normal = NORMALS[next () % 5], y = 1;
      } else {
	ba = b[1], bb = NONE, normal = NORMALS[0], y = 0.3f;
      }
      c.geom.pos[0] = uni (-0.3f, 0.3f);
      c.geom.pos[1] = y + uni (-0.05f, 0.05f);
      c.geom.pos[2] = uni (-0.3f, 0.3f);
      memcpy (c.geom.normal, normal, 3 * 4);
      c.geom.depth = uni (0, 0.05f);
      uint32_t in[23];
      in[0] = (uint32_t) c.surface.mode;
      memcpy (in + 1, &c.surface.mu, 10 * 4);
      memcpy (in + 11, c.geom.pos, 3 * 4);
      memcpy (in + 14, c.geom.normal, 3 * 4);
      memcpy (in + 17, &c.geom.depth, 4);
      memcpy (in + 18, c.fdir1, 3 * 4);
      in[21] = ba == b[0] ? 0 : 1;
      in[22] = bb == b[0] ? 0 : 0xFFFFFFFFu;
      rec (T_CONTACT, in, 23);
      made[i] = (i & 1) ? H->dJointCreateContactSpecial (w, group, &c)
			: H->dJointCreateContact (w, group, &c);
      H->dJointAttach (made[i], ba, bb);
      H->dJointSetFeedback (made[i], &fb[i]);
    }
    H->dWorldStep (w, STEP);
    for (int i = 0; i < 2; i++) rec_body (b[i]);
    for (int i = 0; i < n; i++) rec_floats (T_FEEDBACK, (const dReal *) &fb[i], 16);
    /* as pile_step: emptying the group leaves its joints in the world */
    for (int i = 0; i < n; i++) {
      H->dJointAttach (made[i], NONE, NONE);
      H->dWorldRemoveJoint (w, made[i]);
    }
    H->dJointGroupDestroy (group);
    H->dWorldDestroy (w);
  }
}

/* a body with tiny quaternion components (7), no joint, stepped once:
** rotations about y (cos, sin as written) with tiny entries off it, some of
** them denormal; run with and without denormals-are-zero */
static void s_denormal (void)
{
  static const float base[][2] = {{1, 0}, {0.9238795f, 0.38268343f}, {0.6f, 0.8f},
				  {0.70710677f, 0.70710677f}};
  for (int i = 0; i < 800; i++) {
    float c = base[i % 4][0], s = base[i % 4][1];
    uint32_t t[2], u[2];
    for (int k = 0; k < 2; k++) {
      int e = -40 - (int) (next () % 100);	/* down to below the normal range */
      if (next () % 4 == 0) t[k] = 0;
      else if (e >= -126) t[k] = bits (spread (e, e));
      else t[k] = (next () & 1u) << 31 | (((next () & 0x7FFFFFu) | 0x800000u) >> (-126 - e));
      u[k] = t[k] ^ 0x80000000u;	/* negated, as bits */
    }
    /* entries written, not computed: x about t[0], z about t[1] */
    uint32_t r[12] = {bits (c), u[1], bits (s), 0, t[1], bits (1), u[0], 0,
		      bits (s) ^ 0x80000000u, t[0], bits (c), 0};
    dReal R[12];
    memcpy (R, r, sizeof R);
    dMass m;
    H->dMassSetBoxTotal (&m, uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    dReal lvel[3] = {uni (-1, 1), uni (-1, 1), uni (-1, 1)};
    dReal avel[3] = {0, 0, 0}, torque[3] = {0, 0, 0};
    if ((i / 4) % 3 == 1) avel[1] = uni (-2, 2);
    if ((i / 4) % 3 == 2) for (int k = 0; k < 3; k++) avel[k] = uni (-2, 2);
    step_alone (&m, R, lvel, avel, torque, (i / 12) & 1);
  }
}

/* dNormalize3_slow with two or three largest magnitudes equal (3) */
static void s_normalise_ties (void)
{
  for (int i = 0; i < 2000; i++) {
    int e = (int) (next () % 40) - 30;
    float m = from_bits (bits (spread (e, e)) & 0x7FFFFFFF);
    int pattern = i % 4;	/* the one not largest: 0, 1, 2; or none */
    dReal v[4];
    for (int k = 0; k < 3; k++) {
      float x = k == pattern ? (next () % 8 == 0 ? 0 : from_bits (bits (spread (e - 8, e - 1)) & 0x7FFFFFFF)) : m;
      v[k] = (next () & 1) ? -x : x;
    }
    v[3] = 0;
    dReal in[3] = {v[0], v[1], v[2]};
    H->dNormalize3_slow (v);
    rec_floats (T_VEC, in, 3);
    rec_floats (T_VEC, v, 3);
  }
}

/* ---- the open items of physics-harness-2.md (#885 - #893) ---- */

/* a rotation of rand_rotation's kind, its fourth column zeroed first */
static void posed_rotation (dReal *R)
{
  memset (R, 0, 12 * 4);
  rand_rotation (R);
}

/* step_alone in finite-rotation mode, without torque: about the axis
** (recorded first) when axis, else without one */
static void step_alone_finite (const dMass *m, const dReal *R, const dReal *avel,
			       const dReal *axis, int exact)
{
  dWorldID w = game_world ();
  dBodyID b = H->dBodyCreate (w);
  H->dBodySetMass (b, m);
  H->dBodySetGravityMode (b, 0);
  H->dBodySetPosition (b, 1, 2, 3);
  H->dBodySetRotation (b, R);
  H->dBodySetLinearVel (b, 0.5f, -0.25f, 1);
  H->dBodySetAngularVel (b, avel[0], avel[1], avel[2]);
  H->dBodySetFiniteRotationMode (b, 1);
  if (axis) {
    H->dBodySetFiniteRotationAxis (b, axis[0], axis[1], axis[2]);
    rec_floats (T_VEC, axis, 3);
  }
  rec_mass (m);
  rec_body (b);
  if (exact) H->dWorldStep (w, STEP);
  else H->dWorldQuickStep (w, STEP);
  rec_body (b);
  H->dWorldDestroy (w);
}

/* finite rotation (#885): about an axis along a coordinate axis (either
** sign, unit or not), about a general axis, and without an axis at angles
** (|w| h / 2) below 1e-4 and around it; one step of each stepper */
static void s_finite (void)
{
  for (int i = 0; i < 960; i++) {
    int kind = i % 6, exact = (i / 6) & 1;
    dMass m;
    if ((i / 12) & 1) H->dMassSetSphereTotal (&m, uni (1, 200), uni (0.1f, 1));
    else H->dMassSetBoxTotal (&m, uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    dReal R[12];
    memcpy (R, IDENTITY, sizeof R);
    if ((i / 24) & 1) posed_rotation (R);
    dReal avel[3], axis[3] = {0, 0, 0};
    for (int k = 0; k < 3; k++) avel[k] = uni (-5, 5);
    switch (kind) {
    case 0: case 1: case 2: {
      /* a coordinate axis; sometimes the angular velocity along it alone */
      static const float scale[4] = {1, -1, 2, -0.5f};
      axis[kind] = scale[(i / 48) % 4];
      if ((i / 6) % 4 == 3) for (int k = 0; k < 3; k++) if (k != kind) avel[k] = 0;
      break;
    }
    case 3: /* a general axis */
      axis[0] = uni (-1, 1), axis[1] = uni (-1, 1) + 0.01f, axis[2] = uni (-1, 1);
      break;
    case 4: /* no axis, |w| h / 2 below 1e-4 */
      for (int k = 0; k < 3; k++) avel[k] = spread (-30, -9);
      if (next () % 8 == 0) avel[next () % 3] = 0;
      break;
    default: /* no axis, |w| h / 2 about 1e-4 (|w| about 0.01) */
      for (int k = 0; k < 3; k++) avel[k] = uni (-0.008f, 0.008f);
      break;
    }
    step_alone_finite (&m, R, avel, kind < 4 ? axis : (const dReal *) NONE, exact);
  }
}

/* dMassSetCappedCylinderTotal's dMass (#886): its inputs (the total, the
** direction as an integer, the radius, the length), then the dMass */
static void s_mass_ccylinder (void)
{
  static const float special[][3] = {
    {1, 1, 1}, {10, 0.5f, 2}, {100, 0.3f, 0}, {1, 0.01f, 5}, {50, 2, 0.1f}, {0.5f, 0.25f, 0.75f}
  };
  int nspecial = (int) (sizeof special / sizeof special[0]);
  for (int i = 0; i < 3000; i++) {
    int direction = 1 + i % 3;
    float a[3];
    if (i / 3 < nspecial) memcpy (a, special[i / 3], sizeof a);
    else if (i & 4) {
      a[0] = uni (1, 200), a[1] = uni (0.05f, 1), a[2] = uni (0, 3);
    } else {
      a[0] = from_bits (bits (spread (-4, 12)) & 0x7FFFFFFF);
      a[1] = from_bits (bits (spread (-8, 2)) & 0x7FFFFFFF);
      a[2] = from_bits (bits (spread (-8, 3)) & 0x7FFFFFFF);
    }
    dReal in[4] = {a[0], from_bits ((uint32_t) direction), a[1], a[2]};
    rec_floats (T_VEC, in, 4);
    dMass m;
    memset (&m, 0, sizeof m);
    H->dMassSetCappedCylinderTotal (&m, a[0], direction, a[1], a[2]);
    rec_mass (&m);
  }
}

/* a geometry as read back (23 words): its class, its parameters (a sphere's
** radius; a box's lengths; a capped cylinder's radius and length; a plane's
** four; a ray's length, start and direction), then, but for a plane, its
** position and rotation (the fourth column recorded as zero: a ray's is left
** as the stack held it) */
static void rec_geom_state (dGeomID g)
{
  dReal v[23];
  memset (v, 0, sizeof v);
  int c = H->dGeomGetClass (g);
  v[0] = from_bits ((uint32_t) c);
  switch (c) {
  case 0: v[1] = H->dGeomSphereGetRadius (g); break;
  case 1: H->dGeomBoxGetLengths (g, v + 1); v[4] = 0; break;
  case 2: H->dGeomCCylinderGetParams (g, v + 1, v + 2); break;
  case 4: H->dGeomPlaneGetParams (g, v + 1); break;
  case 5: {
    dReal start[4], dir[4];
    v[1] = H->dGeomRayGetLength (g);
    H->dGeomRayGet (g, start, dir);
    memcpy (v + 2, start, 12);
    memcpy (v + 5, dir, 12);
    break;
  }
  default: break;
  }
  if (c != 4) {
    memcpy (v + 8, H->dGeomGetPosition (g), 12);
    memcpy (v + 11, H->dGeomGetRotation (g), 48);
    v[14] = v[18] = v[22] = 0;
  }
  rec_floats (T_GEOM, v, 23);
}

/* a plane's normal: a Pythagorean quadruple over its length (one rounding
** each), signs and order varied, or three uniform components (the library
** normalises them) */
static void plane_normal (dReal *n)
{
  static const float quad[][4] = {{2, 3, 6, 7}, {1, 4, 8, 9}, {2, 6, 9, 11}, {4, 4, 7, 9}, {0, 3, 4, 5}};
  if (next () % 3 == 0) {
    n[0] = uni (-1, 1), n[1] = uni (-1, 1) + 0.01f, n[2] = uni (-1, 1);
    return;
  }
  const float *q = quad[next () % 5];
  int r = (int) (next () % 3);
  for (int k = 0; k < 3; k++) {
    float x = q[(k + r) % 3] / q[3];
    n[k] = (next () & 1) ? -x : x;
  }
}

/* contacts as pairs records them (the kind, the count, the contacts),
** through dCollide, or the ray's collider `direct' (0 sphere, 1 box, 2
** capped cylinder) */
static void collide_posed (dGeomID a, dGeomID b, uint32_t k, int max, int direct)
{
  int (*fn[3]) (dGeomID, dGeomID, int, dContactGeom *, int) = {
    H->dCollideRaySphere, H->dCollideRayBox, H->dCollideRayCCylinder};
  dContactGeom cg[16];
  int n = direct >= 0 ? fn[direct] (a, b, max, cg, (int) sizeof (dContactGeom))
		      : H->dCollide (a, b, max, cg, (int) sizeof (dContactGeom));
  uint32_t w[2 + 16 * 7];
  w[0] = k;
  w[1] = (uint32_t) n;
  for (int c = 0; c < n && c < 16; c++) {
    for (int e = 0; e < 3; e++) w[2 + c * 7 + e] = bits (cg[c].pos[e]);
    for (int e = 0; e < 3; e++) w[5 + c * 7 + e] = bits (cg[c].normal[e]);
    w[8 + c * 7] = bits (cg[c].depth);
  }
  rec (T_CONTACTS, w, 2 + (uint32_t) (n < 16 ? n : 16) * 7);
}

/* the colliders with every input read back (#887): box-box, sphere-box,
** capped cylinder-box, two capped cylinders, box-plane, sphere-plane,
** capped cylinder-plane, ray-box, ray-capped cylinder, ray-sphere; sizes
** varied, the first geometry away from the origin, planes of general
** normals through its neighbourhood. Each case: both geometries as read
** back, then the contacts at most 1 and at most 8 asked for (rays: 1,
** alternately through the ray's collider and dCollide) */
static void s_pairs_posed (void)
{
  dGeomID sphere = H->dCreateSphere (0, 0.5f), box1 = H->dCreateBox (0, 1, 1, 1);
  dGeomID box2 = H->dCreateBox (0, 1, 1, 1), ccyl1 = H->dCreateCCylinder (0, 0.3f, 1);
  dGeomID ccyl2 = H->dCreateCCylinder (0, 0.25f, 0.8f), plane = H->dCreatePlane (0, 0, 1, 0, 0);
  dGeomID ray = H->dCreateRay (0, 5);
  enum { KINDS = 10 };
  dGeomID first[KINDS] = {box1, sphere, ccyl1, ccyl1, box1, sphere, ccyl1, box1, ccyl1, sphere};
  dGeomID second[KINDS] = {box2, box2, box2, ccyl2, plane, plane, plane, ray, ray, ray};
  for (int i = 0; i < 6000; i++) {
    int k = i % KINDS;
    dGeomID a = first[k], b = second[k];
    H->dGeomBoxSetLengths (box1, uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    H->dGeomBoxSetLengths (box2, uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    H->dGeomSphereSetRadius (sphere, uni (0.05f, 1));
    H->dGeomCCylinderSetParams (ccyl1, uni (0.05f, 0.8f), uni (0, 2));
    H->dGeomCCylinderSetParams (ccyl2, uni (0.05f, 0.8f), uni (0, 2));
    dReal p[3] = {uni (-30, 30), uni (-30, 30), uni (-30, 30)}, R[12];
    H->dGeomSetPosition (a, p[0], p[1], p[2]);
    posed_rotation (R);
    H->dGeomSetRotation (a, R);
    if (b == plane) {
      dReal n[3];
      plane_normal (n);
      float d = n[0] * p[0] + n[1] * p[1] + n[2] * p[2] + uni (-1.2f, 1.2f);
      H->dGeomPlaneSetParams (plane, n[0], n[1], n[2], d);
    } else if (b == ray) {
      float sx = p[0] + uni (-3, 3), sy = p[1] + uni (-3, 3), sz = p[2] + uni (-3, 3);
      H->dGeomRaySet (ray, sx, sy, sz, uni (-1, 1), uni (-1, 1), uni (-1, 1));
    } else {
      float x = p[0] + uni (-1.2f, 1.2f), y = p[1] + uni (-1.2f, 1.2f), z = p[2] + uni (-1.2f, 1.2f);
      H->dGeomSetPosition (b, x, y, z);
      posed_rotation (R);
      H->dGeomSetRotation (b, R);
    }
    rec_geom_state (a);
    rec_geom_state (b);
    /* the ray's colliders: box, capped cylinder, sphere */
    static const int ray_collider[3] = {1, 2, 0};
    if (b == ray) collide_posed (ray, a, (uint32_t) k, 1, (i / KINDS) & 1 ? -1 : ray_collider[k - 7]);
    else for (int max = 1; max <= 8; max += 7) collide_posed (a, b, (uint32_t) k, max, -1);
  }
  H->dGeomDestroy (sphere);
  H->dGeomDestroy (box1);
  H->dGeomDestroy (box2);
  H->dGeomDestroy (ccyl1);
  H->dGeomDestroy (ccyl2);
  H->dGeomDestroy (plane);
  H->dGeomDestroy (ray);
}

/* a contact joint of the piles' kinds made, attached and given feedback,
** its inputs recorded as exact-step's (the bodies as indices into b, ~0
** for the world) */
static dJointID solve_contact (dWorldID w, dJointGroupID group, dBodyID *b, int i, int kind,
			       dBodyID ba, dBodyID bb, const float *normal, float y,
			       dJointFeedback *fb)
{
  dContact c;
  memset (&c, 0, sizeof c);
  c.surface.mode = dContactApprox1 | dContactSoftERP | dContactSoftCFM;
  c.surface.mu = kind == 3 ? 0.3f : 1.0f;
  c.surface.soft_erp = 0.5f;
  c.surface.soft_cfm = 1e-5f;
  if (kind == 1) {
    c.surface.mode |= dContactBounce;
    c.surface.bounce = 0.4f;
    c.surface.bounce_vel = 0.1f;
  }
  if (kind == 2) {
    c.surface.mode |= dContactMu2;
    c.surface.mu2 = 0.5f;
  }
  c.geom.pos[0] = uni (-0.3f, 0.3f);
  c.geom.pos[1] = y + uni (-0.05f, 0.05f);
  c.geom.pos[2] = uni (-0.3f, 0.3f);
  memcpy (c.geom.normal, normal, 3 * 4);
  c.geom.depth = uni (0, 0.05f);
  uint32_t in[23];
  in[0] = (uint32_t) c.surface.mode;
  memcpy (in + 1, &c.surface.mu, 10 * 4);
  memcpy (in + 11, c.geom.pos, 3 * 4);
  memcpy (in + 14, c.geom.normal, 3 * 4);
  memcpy (in + 17, &c.geom.depth, 4);
  memcpy (in + 18, c.fdir1, 3 * 4);
  in[21] = ba == b[0] ? 0 : 1;
  in[22] = bb == NONE ? 0xFFFFFFFFu : bb == b[0] ? 0 : 1;
  rec (T_CONTACT, in, 23);
  dJointID j = (i & 1) ? H->dJointCreateContactSpecial (w, group, &c)
		       : H->dJointCreateContact (w, group, &c);
  H->dJointAttach (j, ba, bb);
  H->dJointSetFeedback (j, fb);
  return j;
}

/* the solvers on their own (#888): one step of QuickStep or the exact
** stepper (recorded as a flag, 1 for the exact) with sphere masses at the
** identity rotation and no angular velocity (exact world-frame inertias, no
** gyroscopic term), and one or two contacts: one body on the ground by one
** or two, or two bodies, the upper on the lower by one, the lower also on
** the ground by a second. Per trial: the masses and bodies, the contacts'
** inputs, the flag, the bodies and the contacts' feedback after */
static void s_solve (void)
{
  for (int t = 0; t < 400; t++) {
    int exact = t & 1, config = (t / 2) % 4, nb = config < 2 ? 1 : 2;
    dWorldID w = game_world ();
    dJointGroupID group = H->dJointGroupCreate (0);
    dBodyID b[2] = {NONE, NONE};
    for (int i = 0; i < nb; i++) {
      b[i] = H->dBodyCreate (w);
      dMass m;
      H->dMassSetSphereTotal (&m, uni (5, 100), uni (0.2f, 1));
      H->dBodySetMass (b[i], &m);
      H->dBodySetPosition (b[i], uni (-0.1f, 0.1f), 0.5f + (float) i, uni (-0.1f, 0.1f));
      H->dBodySetLinearVel (b[i], uni (-1, 1), uni (-1, 1), uni (-1, 1));
      rec_mass (&m);
      rec_body (b[i]);
    }
    static dJointFeedback fb[2];
    memset (fb, 0, sizeof fb);
    dJointID made[2];
    int n = config == 0 || config == 2 ? 1 : 2;
    for (int i = 0; i < n; i++) {
      int kind = (t / 8 + i) % 4;
      const float *normal = NORMALS[0];
      if (config < 2) {
	if (i) normal = NORMALS[next () % 5];
	made[i] = solve_contact (w, group, b, i, kind, b[0], NONE, normal, 0, &fb[i]);
      } else if (i == 0) {
	normal = NORMALS[next () % 5];
	made[i] = solve_contact (w, group, b, i, kind, b[1], b[0], normal, 1, &fb[i]);
      } else {
	made[i] = solve_contact (w, group, b, i, kind, b[0], NONE, normal, 0, &fb[i]);
      }
    }
    rec_int (T_FLAG, (uint32_t) exact);
    if (exact) H->dWorldStep (w, STEP);
    else H->dWorldQuickStep (w, STEP);
    for (int i = 0; i < nb; i++) rec_body (b[i]);
    for (int i = 0; i < n; i++) rec_floats (T_FEEDBACK, (const dReal *) &fb[i], 16);
    for (int i = 0; i < n; i++) {
      H->dJointAttach (made[i], NONE, NONE);
      H->dWorldRemoveJoint (w, made[i]);
    }
    H->dJointGroupDestroy (group);
    H->dWorldDestroy (w);
  }
}

/* a tiny value's bits as s_denormal makes them: zero, or of an exponent
** from -40 down to below the normal range */
static uint32_t tiny_bits (void)
{
  int e = -40 - (int) (next () % 100);
  if (next () % 4 == 0) return 0;
  if (e >= -126) return bits (spread (e, e));
  return (next () & 1u) << 31 | (((next () & 0x7FFFFFu) | 0x800000u) >> (-126 - e));
}

/* rotations about y: (cos, sin) */
static const float ABOUT_Y[][2] = {{1, 0}, {0.9238795f, 0.38268343f}, {0.6f, 0.8f},
				   {0.70710677f, 0.70710677f}, {-0.6f, 0.8f},
				   {-0.9238795f, 0.38268343f}};

/* one body held by a fixed joint to the world (#889) at a pose whose
** quaternion (c, t0, s, t1) has tiny x and z components (written, c and s
** of ABOUT_Y's first four), the body set to that quaternion, to the
** rotation of s_denormal's kind with the same tiny entries, or left at the
** identity; stepped once by either stepper. Per case: the pose's
** quaternion and position, the mass, the body before, then the body and
** the joint's feedback after */
static void s_held (void)
{
  for (int i = 0; i < 600; i++) {
    float c = ABOUT_Y[i % 4][0], s = ABOUT_Y[i % 4][1];
    int set = (i / 4) % 3, exact = (i / 12) & 1;
    uint32_t t[2];
    t[0] = tiny_bits ();
    t[1] = tiny_bits ();
    uint32_t qb[4] = {bits (c), t[0], bits (s), t[1]};
    dReal q[4], pos[4];
    memcpy (q, qb, sizeof q);
    pos[0] = uni (-2, 2);
    pos[1] = uni (1, 6);
    pos[2] = uni (-2, 2);
    pos[3] = 0;
    rec_floats (T_VEC, q, 4);
    rec_floats (T_VEC, pos, 3);
    dWorldID w = game_world ();
    dBodyID b = H->dBodyCreate (w);
    dMass m;
    H->dMassSetBoxTotal (&m, uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    H->dBodySetMass (b, &m);
    H->dBodySetPosition (b, pos[0], pos[1], pos[2]);
    if (set == 0) H->dBodySetQuaternion (b, q);
    else if (set == 1) {
      uint32_t u[2] = {t[0] ^ 0x80000000u, t[1] ^ 0x80000000u};
      uint32_t r[12] = {bits (c), u[1], bits (s), 0, t[1], bits (1), u[0], 0,
			bits (s) ^ 0x80000000u, t[0], bits (c), 0};
      dReal R[12];
      memcpy (R, r, sizeof R);
      H->dBodySetRotation (b, R);
    }
    if ((i / 24) & 1) {
      float x = uni (-2, 2), y = uni (-2, 2), z = uni (-2, 2);
      H->dBodySetAngularVel (b, x, y, z);
    }
    {
      float x = uni (-1, 1), y = uni (-1, 1), z = uni (-1, 1);
      H->dBodySetLinearVel (b, x, y, z);
    }
    dJointID j = H->dJointCreateFixed (w, 0);
    H->dJointAttach (j, b, NONE);
    H->dJointSetFixedQuaternionPos (j, q, pos);
    static dJointFeedback fb;
    memset (&fb, 0, sizeof fb);
    H->dJointSetFeedback (j, &fb);
    rec_mass (&m);
    rec_body (b);
    if (exact) H->dWorldStep (w, STEP);
    else H->dWorldQuickStep (w, STEP);
    rec_body (b);
    rec_floats (T_FEEDBACK, (const dReal *) &fb, 16);
    H->dWorldDestroy (w);
  }
}

/* dBodySetRotation's quaternion where its components fall within a
** rounding of the smallest normal value (#890): rotations about y
** (ABOUT_Y, the last two of a negative trace) with x and z parts written
** of exponents -128 to -121 (the lowest two denormal), equal and opposite
** or one of each pair zero. Per case: the rotation, then the quaternion and
** rotation read back */
static void s_set_rotation (void)
{
  dWorldID w = game_world ();
  dBodyID b = H->dBodyCreate (w);
  for (int i = 0; i < 3000; i++) {
    float c = ABOUT_Y[i % 6][0], s = ABOUT_Y[i % 6][1];
    uint32_t t[2];
    for (int k = 0; k < 2; k++) {
      int e = -128 + (int) (next () % 8);
      uint32_t mant = next () & 0x7FFFFFu;
      t[k] = e >= -126 ? (uint32_t) (e + 127) << 23 | mant : (mant | 0x800000u) >> (-126 - e);
      if (next () & 1) t[k] |= 0x80000000u;
    }
    uint32_t u0 = t[0] ^ 0x80000000u, u1 = t[1] ^ 0x80000000u;
    if ((i / 6) % 3 == 1) u0 = 0;
    if ((i / 6) % 3 == 2) u1 = 0;
    uint32_t r[12] = {bits (c), u1, bits (s), 0, t[1], bits (1), u0, 0,
		      bits (s) ^ 0x80000000u, t[0], bits (c), 0};
    dReal R[12], v[16];
    memcpy (R, r, sizeof R);
    rec_floats (T_ROT, R, 12);
    H->dBodySetRotation (b, R);
    memcpy (v, H->dBodyGetQuaternion (b), 4 * 4);
    memcpy (v + 4, H->dBodyGetRotation (b), 12 * 4);
    rec_floats (T_ROT, v, 16);
  }
  H->dWorldDestroy (w);
}

/* dQFromAxisAndAngle's quaternion for the rotation scenario's axes and
** angles (#891; its seed, so the same inputs), its inputs first; then
** dRFromAxisAndAngle's rotation of them (the fourth column zero),
** dQfromR's quaternion of it, and dGeomGetQuaternion of a geometry without
** a body after dGeomSetRotation of it; traces of both signs */
static void s_quaternion (void)
{
  static const float special[][4] = {
    {1, 0, 0, 1}, {0, 1, 0, 1}, {0, 0, 1, 1}, {2, 0, 0, 1}, {1, 1, 1, 0},
    {1, 1, 0, 3.1415927f}, {0, 0, 1, -3.1415927f}, {1, 2, 3, 0.001f}, {0.5f, 0, 0, 1.5707964f},
    {1, 1, 1, 1}
  };
  int nspecial = (int) (sizeof special / sizeof special[0]);
  dGeomID box = H->dCreateBox (0, 1, 1, 1);
  for (int i = 0; i < 3000; i++) {
    float a[4];
    if (i < nspecial) memcpy (a, special[i], sizeof a);
    else {
      a[0] = uni (-1, 1);
      a[1] = uni (-1, 1);
      a[2] = uni (-1, 1) + 0.01f;
      a[3] = uni (-3.1f, 3.1f);
    }
    dReal v[8], q[4], R[12];
    memcpy (v, a, sizeof a);
    H->dQFromAxisAndAngle (v + 4, a[0], a[1], a[2], a[3]);
    rec_floats (T_VEC, v, 8);
    memset (R, 0, sizeof R);
    H->dRFromAxisAndAngle (R, a[0], a[1], a[2], a[3]);
    rec_floats (T_ROT, R, 12);
    H->dQfromR (q, R);
    rec_floats (T_VEC, q, 4);
    H->dGeomSetRotation (box, R);
    H->dGeomGetQuaternion (box, q);
    rec_floats (T_VEC, q, 4);
  }
  H->dGeomDestroy (box);
}

/* the relative force functions (#892): a placed, rotated body; per case
** each of dBodyAddRelForce, dBodyAddRelTorque, dBodyAddForceAtPos,
** dBodyAddForceAtRelPos, dBodyAddRelForceAtPos and dBodyAddRelForceAtRelPos
** from zeroed accumulators, with a force (or torque) and point of three
** non-zero components. Per case: the body, then per call its inputs and the
** force and torque accumulators */
static void s_rel_force (void)
{
  dWorldID w = game_world ();
  dBodyID b = H->dBodyCreate (w);
  for (int i = 0; i < 1000; i++) {
    dReal R[12];
    posed_rotation (R);
    {
      float x = uni (-5, 5), y = uni (-5, 5), z = uni (-5, 5);
      H->dBodySetPosition (b, x, y, z);
    }
    H->dBodySetRotation (b, R);
    rec_body (b);
    for (int k = 0; k < 6; k++) {
      dReal in[6], out[6];
      for (int e = 0; e < 6; e++) {
	float x = uni (0.05f, 9);
	in[e] = (next () & 1) ? -x : x;
      }
      H->dBodySetForce (b, 0, 0, 0);
      H->dBodySetTorque (b, 0, 0, 0);
      switch (k) {
      case 0: H->dBodyAddRelForce (b, in[0], in[1], in[2]); break;
      case 1: H->dBodyAddRelTorque (b, in[0], in[1], in[2]); break;
      case 2: H->dBodyAddForceAtPos (b, in[0], in[1], in[2], in[3], in[4], in[5]); break;
      case 3: H->dBodyAddForceAtRelPos (b, in[0], in[1], in[2], in[3], in[4], in[5]); break;
      case 4: H->dBodyAddRelForceAtPos (b, in[0], in[1], in[2], in[3], in[4], in[5]); break;
      default: H->dBodyAddRelForceAtRelPos (b, in[0], in[1], in[2], in[3], in[4], in[5]); break;
      }
      memcpy (out, H->dBodyGetForce (b), 12);
      memcpy (out + 3, H->dBodyGetTorque (b), 12);
      rec_floats (T_VEC, in, k < 2 ? 3 : 6);
      rec_floats (T_VEC, out, 6);
    }
  }
  H->dWorldDestroy (w);
}

/* the angular update with non-diagonal inertias (#893): s_angular's kinds
** and steps, the mass first rotated (dMassRotate), translated
** (dMassTranslate), both, or both and a second box's added (dMassAdd)
** before dBodySetMass */
static void s_angular_mass (void)
{
  for (int i = 0; i < 1600; i++) {
    int kind = i % 8, exact = (i / 8) & 1, how = (i / 16) % 4;
    dMass m;
    if (kind < 3) H->dMassSetSphereTotal (&m, uni (1, 200), uni (0.1f, 1));
    else H->dMassSetBoxTotal (&m, uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
    dReal Rm[12];
    posed_rotation (Rm);
    if (how != 1) H->dMassRotate (&m, Rm);
    if (how != 0) {
      float x = uni (-0.5f, 0.5f), y = uni (-0.5f, 0.5f), z = uni (-0.5f, 0.5f);
      H->dMassTranslate (&m, x, y, z);
    }
    if (how == 3) {
      dMass m2;
      H->dMassSetBoxTotal (&m2, uni (1, 200), uni (0.1f, 2), uni (0.1f, 2), uni (0.1f, 2));
      posed_rotation (Rm);
      H->dMassRotate (&m2, Rm);
      float x = uni (-0.5f, 0.5f), y = uni (-0.5f, 0.5f), z = uni (-0.5f, 0.5f);
      H->dMassTranslate (&m2, x, y, z);
      H->dMassAdd (&m, &m2);
    }
    dReal R[12];
    memcpy (R, IDENTITY, sizeof R);
    if (kind == 1 || kind == 4 || kind >= 6) posed_rotation (R);
    dReal lvel[3] = {0, 0, 0}, avel[3] = {0, 0, 0}, torque[3] = {0, 0, 0};
    int axis = (int) (next () % 3);
    switch (kind) {
    case 0: case 1:
      for (int k = 0; k < 3; k++) torque[k] = uni (-1, 1);
      break;
    case 2:
      for (int k = 0; k < 3; k++) avel[k] = uni (-5, 5);
      break;
    case 3: case 4:
      torque[axis] = uni (-1, 1);
      break;
    case 5: case 6:
      avel[axis] = uni (-5, 5);
      break;
    default:
      for (int k = 0; k < 3; k++) avel[k] = uni (-5, 5);
      break;
    }
    step_alone (&m, R, lvel, avel, torque, exact);
  }
}

typedef struct Scenario {
  const char *name;
  void (*run) (void);
  int nodaz;
  uint32_t seed;
} Scenario;

static const Scenario SCENARIOS[] = {
  {"library", s_library, 0, 1},
  {"x87", s_x87, 0, 2},
  {"normalise", s_normalise, 0, 3},
  {"free", s_free, 0, 4},
  {"joints", s_joints, 0, 5},
  {"contacts", s_contacts, 0, 6},
  {"contacts-exact", s_contacts_exact, 0, 7},
  {"heavy", s_heavy, 0, 8},
  {"pairs", s_pairs, 0, 9},
  {"rays", s_rays, 0, 10},
  {"worlds", s_worlds, 0, 11},
  /* the same inputs as joints and contacts */
  {"joints-nodaz", s_joints, 1, 5},
  {"contacts-nodaz", s_contacts, 1, 6},
  /* the open questions of docs/behavior/physics-harness.md */
  {"angular", s_angular, 0, 12},
  {"rotation", s_rotation, 0, 13},
  {"exact-step", s_exact_step, 0, 14},
  {"normalise-ties", s_normalise_ties, 0, 15},
  {"denormal", s_denormal, 0, 16},
  {"denormal-nodaz", s_denormal, 1, 16},
  /* the open items of docs/behavior/physics-harness-2.md (#885 - #893) */
  {"finite", s_finite, 0, 17},
  {"mass-ccylinder", s_mass_ccylinder, 0, 18},
  {"pairs-posed", s_pairs_posed, 0, 19},
  {"solve", s_solve, 0, 20},
  {"held", s_held, 0, 21},
  {"held-nodaz", s_held, 1, 21},
  {"set-rotation", s_set_rotation, 0, 22},
  {"quaternion", s_quaternion, 0, 13},
  {"rel-force", s_rel_force, 0, 23},
  {"angular-mass", s_angular_mass, 0, 24},
};

int volya_ode_scenario_count (void)
{
  return (int) (sizeof SCENARIOS / sizeof SCENARIOS[0]);
}

const char *volya_ode_scenario_name (int i)
{
  return SCENARIOS[i].name;
}

void volya_ode_scenario_run (const OdeHarness *h, int i)
{
  H = h;
  seed = SCENARIOS[i].seed;
  /* 24-bit precision, round to nearest, exceptions masked; MXCSR masked,
  ** with flush-to-zero and denormals-are-zero unless nodaz */
  h->fpenv (0x007F, SCENARIOS[i].nodaz ? 0x1F80 : 0x9FC0);
  h->srand (1);
  SCENARIOS[i].run ();
}

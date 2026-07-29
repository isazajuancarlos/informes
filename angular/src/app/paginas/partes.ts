/**
 * Editor de participación de un informe: quién interviene y con qué papel.
 *
 * Es el prerequisito de todo el flujo (#162): de aquí salen el **responsable**
 * de las entregas (EJECUTA/ORIGEN) y el **destino externo** de las alertas
 * (DESTINO). Un informe sin partes genera obligaciones que nadie atiende y de las
 * que nadie se entera.
 *
 * El guardado **reemplaza** el conjunto completo: lo que se ve es lo que queda.
 */
import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { ActivatedRoute, Router } from '@angular/router';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatListModule } from '@angular/material/list';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import { firstValueFrom } from 'rxjs';

import { Api, OpcionParte, Parte, RolParte } from '../nucleo/api';

/** Una parte en edición, con el nombre resuelto para pintarla. */
interface ParteEdit {
  usuario_id: number | null;
  stakeholder_id: number | null;
  rol: RolParte;
  nombre: string;
}

const ROLES: { valor: RolParte; etiqueta: string }[] = [
  { valor: 'EJECUTA', etiqueta: 'Responsable (ejecuta)' },
  { valor: 'ORIGEN', etiqueta: 'Origen' },
  { valor: 'SUPERVISA', etiqueta: 'Supervisa' },
  { valor: 'USUARIO', etiqueta: 'Usuario' },
  { valor: 'DESTINO', etiqueta: 'Destino (recibe)' },
];

@Component({
  selector: 'siger-partes',
  imports: [
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatListModule,
    MatProgressBarModule,
    MatSelectModule,
  ],
  template: `
    <header class="cabecera">
      <button mat-icon-button (click)="volver()" aria-label="Volver"><mat-icon>arrow_back</mat-icon></button>
      <h2>Partes de «{{ nombreInforme() }}»</h2>
    </header>
    <p class="guia">
      Quién interviene y con qué papel. El <strong>Responsable</strong> es a quien
      se le asignan las entregas; el <strong>Destino</strong> es el tercero que las
      recibe y a quien avisan las alertas.
    </p>

    @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }
    @if (error()) { <p class="error"><mat-icon>error_outline</mat-icon> {{ error() }}</p> }

    <mat-card>
      <mat-card-content>
        @if (partes().length === 0) {
          <p class="vacio">Este informe todavía no tiene partes. Añada al menos un responsable.</p>
        } @else {
          <mat-list>
            @for (p of partes(); track $index) {
              <mat-list-item>
                <mat-icon matListItemIcon>{{ p.usuario_id ? 'person' : 'apartment' }}</mat-icon>
                <span matListItemTitle>{{ p.nombre }}</span>
                <span matListItemLine>{{ etiquetaRol(p.rol) }} · {{ p.usuario_id ? 'interno' : 'externo' }}</span>
                <button mat-icon-button (click)="quitar($index)" aria-label="Quitar"><mat-icon>close</mat-icon></button>
              </mat-list-item>
            }
          </mat-list>
        }

        <!-- Alta de una parte -->
        <div class="alta">
          <mat-form-field appearance="outline" subscriptSizing="dynamic">
            <mat-label>Tipo</mat-label>
            <mat-select [(ngModel)]="tipo" (ngModelChange)="seleccion.set(null)">
              <mat-option value="interno">Usuario interno</mat-option>
              <mat-option value="externo">Stakeholder externo</mat-option>
            </mat-select>
          </mat-form-field>

          <mat-form-field appearance="outline" subscriptSizing="dynamic">
            <mat-label>{{ tipo() === 'interno' ? 'Usuario' : 'Stakeholder' }}</mat-label>
            <mat-select [(ngModel)]="seleccion">
              @for (o of opciones(); track o.id) {
                <mat-option [value]="o.id">{{ o.nombre }}{{ o.detalle ? ' · ' + o.detalle : '' }}</mat-option>
              }
            </mat-select>
          </mat-form-field>

          <mat-form-field appearance="outline" subscriptSizing="dynamic">
            <mat-label>Papel</mat-label>
            <mat-select [(ngModel)]="rol">
              @for (r of rolesDisponibles(); track r.valor) {
                <mat-option [value]="r.valor">{{ r.etiqueta }}</mat-option>
              }
            </mat-select>
          </mat-form-field>

          <button mat-stroked-button (click)="agregar()" [disabled]="seleccion() === null">
            <mat-icon>add</mat-icon> Añadir
          </button>
        </div>

        @if (mensaje()) { <p class="ok"><mat-icon>check_circle</mat-icon> {{ mensaje() }}</p> }

        <div class="acciones">
          <button mat-flat-button (click)="guardar()" [disabled]="guardando()">
            <mat-icon>save</mat-icon> Guardar partes
          </button>
        </div>
      </mat-card-content>
    </mat-card>
  `,
  styles: `
    .cabecera { display: flex; align-items: center; gap: .5rem; }
    .cabecera h2 { margin: 0; }
    .guia { color: var(--mat-sys-on-surface-variant); margin-top: -.25rem; }
    .alta { display: flex; gap: .75rem; flex-wrap: wrap; align-items: center; margin-top: 1rem; }
    .alta mat-form-field { flex: 1 1 12rem; }
    .acciones { margin-top: 1rem; }
    .error { display: flex; align-items: center; gap: .4rem; color: var(--mat-sys-error); }
    .ok { display: flex; align-items: center; gap: .4rem; color: var(--mat-sys-primary); }
    .vacio { color: var(--mat-sys-on-surface-variant); }
  `,
})
export class Partes {
  private api = inject(Api);
  private router = inject(Router);

  readonly roles = ROLES;
  private readonly id = Number(inject(ActivatedRoute).snapshot.paramMap.get('id'));

  nombreInforme = signal('');
  partes = signal<ParteEdit[]>([]);
  usuarios = signal<OpcionParte[]>([]);
  stakeholders = signal<OpcionParte[]>([]);
  cargando = signal(true);
  guardando = signal(false);
  error = signal('');
  mensaje = signal('');

  tipo = signal<'interno' | 'externo'>('interno');
  seleccion = signal<number | null>(null);
  rol = signal<RolParte>('EJECUTA');

  readonly opciones = computed(() =>
    this.tipo() === 'interno' ? this.usuarios() : this.stakeholders(),
  );
  /** El destino es de externos; los demás papeles, de internos. No se impone —el
   *  servidor acepta cualquiera— pero se guía para evitar el error común. */
  readonly rolesDisponibles = computed(() =>
    this.tipo() === 'externo'
      ? ROLES.filter((r) => r.valor === 'DESTINO')
      : ROLES.filter((r) => r.valor !== 'DESTINO'),
  );

  constructor() {
    void this.cargar();
  }

  etiquetaRol(r: RolParte) {
    return ROLES.find((x) => x.valor === r)?.etiqueta ?? r;
  }

  private async cargar() {
    this.cargando.set(true);
    this.error.set('');
    try {
      const [detalle, partes, us, sk] = await Promise.all([
        firstValueFrom(this.api.informeDetalle(this.id)),
        firstValueFrom(this.api.participacion(this.id)),
        firstValueFrom(this.api.usuarios()),
        firstValueFrom(this.api.stakeholders()),
      ]);
      this.nombreInforme.set(detalle.nombre);
      this.usuarios.set(us);
      this.stakeholders.set(sk);
      this.partes.set(
        partes.map((p: Parte) => ({
          usuario_id: p.usuario_id,
          stakeholder_id: p.stakeholder_id,
          rol: p.rol,
          nombre: p.nombre,
        })),
      );
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  agregar() {
    const idSel = this.seleccion();
    if (idSel === null) return;
    const interno = this.tipo() === 'interno';
    const opcion = this.opciones().find((o) => o.id === idSel);
    if (!opcion) return;
    // El papel Destino solo tiene sentido para externos, y viceversa: si el papel
    // elegido ya no está disponible para el tipo, se corrige al primero válido.
    const rol = this.rolesDisponibles().some((r) => r.valor === this.rol())
      ? this.rol()
      : this.rolesDisponibles()[0].valor;
    this.partes.update((xs) => [
      ...xs,
      {
        usuario_id: interno ? idSel : null,
        stakeholder_id: interno ? null : idSel,
        rol,
        nombre: opcion.nombre,
      },
    ]);
    this.seleccion.set(null);
    this.mensaje.set('');
  }

  quitar(i: number) {
    this.partes.update((xs) => xs.filter((_, idx) => idx !== i));
    this.mensaje.set('');
  }

  async guardar() {
    this.guardando.set(true);
    this.error.set('');
    this.mensaje.set('');
    try {
      const r = await firstValueFrom(
        this.api.guardarParticipacion(
          this.id,
          this.partes().map((p) => ({
            usuario_id: p.usuario_id,
            stakeholder_id: p.stakeholder_id,
            rol: p.rol,
          })),
        ),
      );
      this.mensaje.set(`Guardado: ${r.partes} parte(s).`);
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.guardando.set(false);
    }
  }

  volver() {
    void this.router.navigate(['/informes']);
  }
}
